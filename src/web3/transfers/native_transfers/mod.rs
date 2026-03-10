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
/// Uses alloy's built-in gas estimation. The provider automatically handles
/// EIP-1559 vs legacy transaction types based on the network.
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

    let gas_estimate = provider.estimate_gas(estimation_tx).await?;
    let gas_price = provider.get_gas_price().await?;

    let max_gas_cost = U256::from(gas_estimate) * U256::from(gas_price);
    let value = balance.saturating_sub(max_gas_cost);

    if value.is_zero() {
        return Err(TransferError::InsufficientBalance);
    }

    let tx = TransactionRequest::default()
        .from(invoice.to)
        .to(gateway.config.treasury_address)
        .value(value)
        .gas_limit(gas_estimate);

    let pending = provider.send_transaction(tx).await?;

    let receipt = pending
        .with_required_confirmations(gateway.config.min_confirmations)
        .get_receipt()
        .await?;

    Ok(format!("{:?}", receipt.transaction_hash))
}
