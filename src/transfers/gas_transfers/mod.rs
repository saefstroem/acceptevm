
use ethers::{ middleware::SignerMiddleware, providers::Middleware, signers::LocalWallet, types::{TransactionRequest, U256}};
use std::ops::Mul;
use crate::{
    gateway::{PaymentGateway, PaymentGatewayConfiguration},
    types::Invoice,
};

use super::{errors::TransferError, get_chain_id, get_gas_price};


/// Creates a transaction to transfer gas from a paid invoice to a specified treasury address
async fn create_transaction(
    gateway_config: PaymentGatewayConfiguration,
    invoice: &Invoice,
    chain_id: U256,
    gas_price: U256,
) -> TransactionRequest {

    // Use specified gas limit or fallback
    let gas_limit = gateway_config.transfer_gas_limit.unwrap_or(21000);

    // Maximum cost of transaction
    let max_cost = gas_limit.mul(gas_price.as_u128());

    // Estimated gas left after transfer
    let value = invoice.amount.saturating_sub(U256::from(max_cost));

    TransactionRequest::default()
        .from(invoice.to)
        .to(gateway_config.treasury_address)
        .nonce(0)
        .chain_id(chain_id.as_u64())
        .gas(gas_limit)
        .value(value)
        .gas_price(gas_price)   
}



/// Transfers gas from a paid invoice to a specified treasury address
pub async fn transfer_gas_to_treasury(
    gateway: PaymentGateway,
    invoice: &Invoice,
) -> Result<String, TransferError> {
    let signer = LocalWallet::from_bytes(&invoice.wallet).unwrap();
    let chain_id = get_chain_id(gateway.config.provider.clone()).await?;
    let gas_price = get_gas_price(gateway.config.provider.clone()).await?;

    let transaction = create_transaction(gateway.config.clone(), invoice, chain_id, gas_price).await;
    let client = SignerMiddleware::new(gateway.config.provider, signer);

    let pending_tx = client
        .send_transaction(transaction, None)
        .await
        .map_err(|e| {
            log::error!("Could not send transaction: {}", e);
            TransferError::SendTransaction
        })?;

    let receipt = pending_tx
        .confirmations(gateway.config.min_confirmations)
        .await  
        .map_err(|e| {
            log::error!("Error waiting for confirmations: {}", e);
            TransferError::TransactionNotConfirmed
        })?
        .ok_or_else(|| {
            log::error!("Transaction not confirmed");
            TransferError::TransactionNotConfirmed
        })?;

    log::info!("Transaction confirmed: {:?}", receipt.transaction_hash);
    Ok(format!("{:?}", receipt.transaction_hash))
}