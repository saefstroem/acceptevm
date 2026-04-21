use alloy::network::EthereumWallet;
use alloy::primitives::{B256, U256};
use alloy::providers::{Provider, ProviderBuilder};
use alloy::rpc::types::TransactionRequest;
use alloy::signers::local::PrivateKeySigner;

use crate::gateway::PaymentGateway;
use crate::invoice::Invoice;
use crate::web3::error::TransferError;
use crate::web3::result::Result;

/// Replacement transactions must pay at least 10% higher fees to be accepted
/// by the mempool (EIP-1559 / legacy). Expressed as a fraction: 11/10 = 110%.
const FEE_BUMP_NUMERATOR: u128 = 11;
const FEE_BUMP_DENOMINATOR: u128 = 10;

fn bump_fee(fee: u128) -> u128 {
    let bumped = fee
        .saturating_mul(FEE_BUMP_NUMERATOR)
        .saturating_add(FEE_BUMP_DENOMINATOR - 1)
        / FEE_BUMP_DENOMINATOR;
    if bumped <= fee {
        fee.saturating_add(1)
    } else {
        bumped
    }
}

/// Sends the full native-token balance from a paid invoice's wallet to the
/// treasury, minus gas costs.
///
/// Returns `(tx_hash, nonce)` immediately after broadcasting — does NOT wait
/// for on-chain confirmation. When `invoice.nonce` is set this is a
/// replacement tx that reuses the same nonce with bumped fees.
pub async fn send_native_to_treasury(
    gateway: &PaymentGateway,
    invoice: &Invoice,
) -> Result<(String, u64)> {
    let key_bytes: [u8; 32] = invoice.wallet.inner.as_slice().try_into()?;
    let signer = PrivateKeySigner::from_bytes(&key_bytes.into())?;
    let wallet = EthereumWallet::from(signer);

    let provider = ProviderBuilder::new()
        .wallet(wallet)
        .connect_http(gateway.next_rpc_url().parse()?);

    let balance = provider.get_balance(invoice.to).await?;
    if balance.is_zero() {
        return Err(TransferError::InsufficientBalance);
    }

    let nonce = match invoice.nonce {
        Some(n) => n,
        None => provider.get_transaction_count(invoice.to).await?,
    };

    // Estimate gas with a zero-value tx — the actual value is set after we
    // know the total gas cost so we can send `balance - gas_cost`.
    let gas_limit = provider
        .estimate_gas(
            TransactionRequest::default()
                .from(invoice.to)
                .to(gateway.config.treasury_address)
                .value(U256::ZERO),
        )
        .await?;

    let is_replacement = invoice.nonce.is_some();
    let treasury = gateway.config.treasury_address;

    let (max_gas_cost, tx) = build_tx(
        &provider,
        invoice,
        treasury,
        balance,
        gas_limit,
        nonce,
        is_replacement,
    )
    .await?;

    // After subtracting gas there must be something left to actually send.
    if balance.saturating_sub(max_gas_cost).is_zero() {
        return Err(TransferError::InsufficientBalance);
    }

    let pending = provider.send_transaction(tx).await?;
    Ok((format!("{:?}", pending.tx_hash()), nonce))
}

/// Builds the treasury transfer tx, trying EIP-1559 fee estimation first and
/// falling back to legacy gas pricing if the network doesn't support it.
///
/// The transfer value is set to `balance - gas_cost` so the entire wallet is
/// drained. Replacement txs get a 10% fee bump to satisfy mempool rules.
async fn build_tx(
    provider: &impl Provider,
    invoice: &Invoice,
    treasury: alloy::primitives::Address,
    balance: U256,
    gas_limit: u64,
    nonce: u64,
    is_replacement: bool,
) -> Result<(U256, TransactionRequest)> {
    let base = TransactionRequest::default()
        .from(invoice.to)
        .to(treasury)
        .gas_limit(gas_limit)
        .nonce(nonce);

    match provider.estimate_eip1559_fees().await {
        Ok(eip1559) => {
            let max_fee = if is_replacement {
                bump_fee(eip1559.max_fee_per_gas)
            } else {
                eip1559.max_fee_per_gas
            };
            let priority = if is_replacement {
                bump_fee(eip1559.max_priority_fee_per_gas)
            } else {
                eip1559.max_priority_fee_per_gas
            };
            let cost = U256::from(gas_limit) * U256::from(max_fee);

            Ok((
                cost,
                base.value(balance.saturating_sub(cost))
                    .max_fee_per_gas(max_fee)
                    .max_priority_fee_per_gas(priority),
            ))
        }
        Err(e) => {
            tracing::warn!("EIP-1559 estimation failed, falling back to legacy: {e}");

            let gas_price = if is_replacement {
                bump_fee(provider.get_gas_price().await?)
            } else {
                provider.get_gas_price().await?
            };
            let cost = U256::from(gas_limit) * U256::from(gas_price);

            Ok((
                cost,
                base.value(balance.saturating_sub(cost))
                    .gas_price(gas_price),
            ))
        }
    }
}

/// Checks whether a previously broadcast treasury transfer has been confirmed
/// with sufficient block depth (`min_confirmations` from config).
///
/// All RPC calls are wrapped in a timeout to prevent hanging on unresponsive
/// nodes. Returns `Ok(false)` on any timeout or transient error so the poller
/// retries on the next cycle.
///
/// After reaching the required depth the receipt is re-fetched to guard
/// against block reorgs that could silently drop the transaction.
pub async fn confirm_treasury_transfer(
    gateway: &PaymentGateway,
    tx_hash_str: &str,
) -> Result<bool> {
    let hash: B256 = tx_hash_str.parse().map_err(|e| {
        tracing::error!("Invalid transaction hash '{tx_hash_str}': {e}");
        TransferError::InvalidTxHash
    })?;

    let provider = ProviderBuilder::new().connect_http(gateway.next_rpc_url().parse()?);
    let timeout = std::time::Duration::from_secs(gateway.config.receipt_timeout_seconds);

    // Step 1: fetch the receipt
    let receipt = match timed(&timeout, provider.get_transaction_receipt(hash)).await {
        Some(Ok(Some(r))) => r,
        Some(Ok(None)) => return Ok(false),
        Some(Err(e)) => {
            tracing::error!("Error fetching receipt for {tx_hash_str}: {e}");
            return Ok(false);
        }
        None => {
            tracing::warn!("Receipt check timed out for {tx_hash_str}");
            return Ok(false);
        }
    };

    // Step 2: check confirmation depth
    let tx_block = match receipt.block_number {
        Some(block) => block,
        None => return Ok(false),
    };

    let latest_block = match timed(&timeout, provider.get_block_number()).await {
        Some(Ok(block)) => block,
        Some(Err(e)) => {
            tracing::error!("Error fetching latest block number: {e}");
            return Ok(false);
        }
        None => {
            tracing::warn!("Block number fetch timed out");
            return Ok(false);
        }
    };

    if latest_block.saturating_sub(tx_block) < gateway.config.min_confirmations {
        return Ok(false);
    }

    // Step 3: re-fetch receipt to ensure it survived potential reorgs
    match timed(&timeout, provider.get_transaction_receipt(hash)).await {
        Some(Ok(Some(_))) => Ok(true),
        Some(Ok(None)) => {
            tracing::warn!("Receipt for {tx_hash_str} disappeared after reorg");
            Ok(false)
        }
        Some(Err(e)) => {
            tracing::error!("Error re-fetching receipt for {tx_hash_str}: {e}");
            Ok(false)
        }
        None => {
            tracing::warn!("Receipt re-fetch timed out for {tx_hash_str}");
            Ok(false)
        }
    }
}

/// Wraps a future in a timeout, returning `None` on expiry instead of a
/// nested `Result<Result<T>, Elapsed>`.
async fn timed<F: std::future::Future>(timeout: &std::time::Duration, fut: F) -> Option<F::Output> {
    tokio::time::timeout(*timeout, fut).await.ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bump_fee_ten_percent_increase() {
        assert_eq!(bump_fee(1000), 1100);
    }

    #[test]
    fn bump_fee_zero_becomes_one() {
        // The implementation guarantees the bumped fee is always strictly
        // greater than the original, so bump_fee(0) == 1.
        assert_eq!(bump_fee(0), 1);
    }

    #[test]
    fn bump_fee_one_gwei() {
        let one_gwei: u128 = 1_000_000_000;
        // 1_000_000_000 * 11 / 10 = 1_100_000_000
        assert_eq!(bump_fee(one_gwei), 1_100_000_000);
    }

    #[test]
    fn bump_fee_large_value_no_overflow() {
        // u128::MAX / 11 is still representable after the multiply
        let val: u128 = u128::MAX / 20;
        let bumped = bump_fee(val);
        assert!(bumped > val, "bumped fee must be larger than original");
    }

    #[test]
    fn bump_fee_idempotent_numerics() {
        // bump_fee uses ceiling division: (fee*11 + 9) / 10
        // bump_fee(10)  = (110 + 9) / 10 = 119 / 10 = 11
        // bump_fee(11)  = (121 + 9) / 10 = 130 / 10 = 13
        let first = bump_fee(10);
        let second = bump_fee(first);
        assert_eq!(first, 11);
        assert_eq!(second, 13);
    }
}
