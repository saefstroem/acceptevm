use alloy::network::EthereumWallet;
use alloy::primitives::U256;
use alloy::providers::{Provider, ProviderBuilder};
use alloy::rpc::types::TransactionRequest;
use alloy::signers::local::PrivateKeySigner;

use crate::gateway::PaymentGateway;
use crate::invoice::Invoice;
use crate::web3::error::TransferError;
use crate::web3::result::Result;

/// Transfers the native token balance from a paid invoice's wallet to the treasury.
///
/// Tries EIP-1559 fee estimation first. If the network does not support it,
/// falls back to legacy gas pricing. This ensures compatibility with all
/// EVM-compatible networks.
pub async fn transfer_native_to_treasury(
    gateway: PaymentGateway,
    invoice: &Invoice,
) -> Result<String> {
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

    let estimation_tx = TransactionRequest::default()
        .from(invoice.to)
        .to(gateway.config.treasury_address)
        .value(U256::ZERO);

    let gas_limit = provider.estimate_gas(estimation_tx).await?;

    // Try EIP-1559 first, fall back to legacy gas price
    let (max_gas_cost, tx) = match provider.estimate_eip1559_fees().await {
        Ok(eip1559) => {
            let cost = U256::from(gas_limit) * U256::from(eip1559.max_fee_per_gas);
            let value = balance.saturating_sub(cost);
            let tx = TransactionRequest::default()
                .from(invoice.to)
                .to(gateway.config.treasury_address)
                .value(value)
                .gas_limit(gas_limit)
                .max_fee_per_gas(eip1559.max_fee_per_gas)
                .max_priority_fee_per_gas(eip1559.max_priority_fee_per_gas);
            (cost, tx)
        }
        Err(e) => {
            tracing::warn!("EIP-1559 fee estimation failed, falling back to legacy gas price: {}", e);
            let gas_price = provider.get_gas_price().await?;
            let cost = U256::from(gas_limit) * U256::from(gas_price);
            let value = balance.saturating_sub(cost);
            let tx = TransactionRequest::default()
                .from(invoice.to)
                .to(gateway.config.treasury_address)
                .value(value)
                .gas_limit(gas_limit)
                .gas_price(gas_price);
            (cost, tx)
        }
    };

    if balance.saturating_sub(max_gas_cost).is_zero() {
        return Err(TransferError::InsufficientBalance);
    }

    let pending = provider.send_transaction(tx).await?;

    let receipt = pending
        .with_required_confirmations(gateway.config.min_confirmations)
        .get_receipt()
        .await?;

    Ok(format!("{:?}", receipt.transaction_hash))
}
