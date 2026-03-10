use alloy::network::EthereumWallet;
use alloy::primitives::{B256, U256};
use alloy::providers::{Provider, ProviderBuilder};
use alloy::rpc::types::TransactionRequest;
use alloy::signers::local::PrivateKeySigner;

use crate::gateway::PaymentGateway;
use crate::invoice::Invoice;
use crate::web3::error::TransferError;
use crate::web3::result::Result;

/// Fee bump multiplier numerator (110% = 11/10) for replacement transactions.
const FEE_BUMP_NUMERATOR: u128 = 11;
const FEE_BUMP_DENOMINATOR: u128 = 10;

/// Sends the native token balance from a paid invoice's wallet to the treasury.
///
/// Returns `(tx_hash, nonce)` immediately after broadcasting. Does NOT wait
/// for confirmation. If `invoice.nonce` is set, this is a replacement tx
/// that reuses the same nonce with bumped fees.
///
/// Tries EIP-1559 fee estimation first. If the network does not support it,
/// falls back to legacy gas pricing.
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

    let estimation_tx = TransactionRequest::default()
        .from(invoice.to)
        .to(gateway.config.treasury_address)
        .value(U256::ZERO);

    let gas_limit = provider.estimate_gas(estimation_tx).await?;
    let is_replacement = invoice.nonce.is_some();

    // Try EIP-1559 first, fall back to legacy gas price
    let (max_gas_cost, tx) = match provider.estimate_eip1559_fees().await {
        Ok(eip1559) => {
            let mut max_fee = eip1559.max_fee_per_gas;
            let mut priority_fee = eip1559.max_priority_fee_per_gas;

            if is_replacement {
                max_fee = max_fee * FEE_BUMP_NUMERATOR / FEE_BUMP_DENOMINATOR;
                priority_fee = priority_fee * FEE_BUMP_NUMERATOR / FEE_BUMP_DENOMINATOR;
            }

            let cost = U256::from(gas_limit) * U256::from(max_fee);
            let value = balance.saturating_sub(cost);
            let tx = TransactionRequest::default()
                .from(invoice.to)
                .to(gateway.config.treasury_address)
                .value(value)
                .gas_limit(gas_limit)
                .nonce(nonce)
                .max_fee_per_gas(max_fee)
                .max_priority_fee_per_gas(priority_fee);
            (cost, tx)
        }
        Err(e) => {
            tracing::warn!(
                "EIP-1559 fee estimation failed, falling back to legacy gas price: {}",
                e
            );
            let mut gas_price = provider.get_gas_price().await?;

            if is_replacement {
                gas_price = gas_price * FEE_BUMP_NUMERATOR / FEE_BUMP_DENOMINATOR;
            }

            let cost = U256::from(gas_limit) * U256::from(gas_price);
            let value = balance.saturating_sub(cost);
            let tx = TransactionRequest::default()
                .from(invoice.to)
                .to(gateway.config.treasury_address)
                .value(value)
                .gas_limit(gas_limit)
                .nonce(nonce)
                .gas_price(gas_price);
            (cost, tx)
        }
    };

    if balance.saturating_sub(max_gas_cost).is_zero() {
        return Err(TransferError::InsufficientBalance);
    }

    let pending = provider.send_transaction(tx).await?;
    let tx_hash = format!("{:?}", pending.tx_hash());

    Ok((tx_hash, nonce))
}

/// Checks whether a previously sent treasury transfer has been confirmed.
///
/// Uses `tokio::time::timeout` with `receipt_timeout_seconds` from config
/// to prevent hanging on unresponsive RPCs.
///
/// Returns `Ok(true)` if confirmed, `Ok(false)` if not yet mined or timed out.
pub async fn confirm_treasury_transfer(
    gateway: &PaymentGateway,
    tx_hash_str: &str,
) -> Result<bool> {
    let hash: B256 = tx_hash_str
        .parse()
        .map_err(|e| {
            tracing::error!("Invalid transaction hash '{}': {}", tx_hash_str, e);
            TransferError::InvalidTxHash
        })?;

    let provider = ProviderBuilder::new().connect_http(gateway.next_rpc_url().parse()?);

    let timeout_duration =
        std::time::Duration::from_secs(gateway.config.receipt_timeout_seconds);

    let receipt_result = tokio::time::timeout(
        timeout_duration,
        provider.get_transaction_receipt(hash),
    )
    .await;

    match receipt_result {
        Ok(Ok(Some(_receipt))) => Ok(true),
        Ok(Ok(None)) => Ok(false),
        Ok(Err(e)) => {
            tracing::error!("Error fetching receipt for {}: {}", tx_hash_str, e);
            Ok(false)
        }
        Err(_) => {
            tracing::warn!("Receipt check timed out for {}", tx_hash_str);
            Ok(false)
        }
    }
}
