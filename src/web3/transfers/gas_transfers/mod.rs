use ethers::{
    middleware::SignerMiddleware,
    providers::{Http, Middleware, Provider},
    signers::{LocalWallet, Signer},
    types::{
        transaction::eip2718::TypedTransaction, BlockId, BlockNumber, Eip1559TransactionRequest,
        TransactionRequest, U256,
    },
};
use std::ops::Mul;

use crate::{
    gateway::{PaymentGateway, TransactionType},
    invoice::Invoice,
    web3::{
        estimate_eip1559_fees_with_retry, get_chain_id, get_gas_price, get_native_balance,
        TransferError,
    },
};

async fn transmit_transaction(
    signer: LocalWallet,
    transaction: TypedTransaction,
    chain_id: U256,
    gateway: PaymentGateway,
) -> Result<String, TransferError> {
    let client = SignerMiddleware::new(
        gateway.config.provider,
        signer.with_chain_id(chain_id.as_u64()),
    );

    let pending_tx = client
        .send_transaction(transaction, Some(BlockId::Number(BlockNumber::Latest)))
        .await
        .map_err(|e| {
            log::error!("Transaction send failed: {}", e);
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

async fn estimate_gas_on_transaction(
    provider: &Provider<Http>,
    transaction: TransactionRequest,
) -> Result<U256, TransferError> {
    provider
        .estimate_gas(
            &transaction.clone().into(),
            Some(BlockId::Number(BlockNumber::Latest)),
        )
        .await
        .map_err(|e| {
            log::error!("Gas estimation failed: {}", e);
            TransferError::SendTransaction
        })
}

async fn transfer_gas_to_treasury_legacy(
    gateway: PaymentGateway,
    invoice: &Invoice,
    signer: LocalWallet,
    chain_id: U256,
    gas_price: U256,
    balance: U256,
) -> Result<String, TransferError> {
    let provider = &gateway.config.provider;

    // Use specified gas limit or fallback during estimation only
    let mut gas_limit = 21000;

    // Maximum cost of transaction
    let mut max_cost = gas_limit.mul(gas_price.as_u128());
    let mut value = balance.saturating_sub(U256::from(max_cost));

    // Create a transaction request
    let mut transaction = TransactionRequest::new()
        .from(invoice.to)
        .to(gateway.config.treasury_address)
        .nonce(0)
        .chain_id(chain_id.as_u64())
        .gas_price(gas_price)
        .value(value);

    // Estimate gas
    let gas_estimate = estimate_gas_on_transaction(provider, transaction.clone()).await?;

    // Update gas limit and max cost
    gas_limit = gas_estimate.as_u128();
    max_cost = gas_limit * gas_price.as_u128();
    value = balance.saturating_sub(U256::from(max_cost));
    transaction = transaction.gas(gas_estimate).value(value);

    // Transmit transaction
    transmit_transaction(signer, transaction.into(), chain_id, gateway).await
}
async fn transfer_gas_to_treasury_eip1559(
    gateway: PaymentGateway,
    invoice: &Invoice,
    signer: LocalWallet,
    chain_id: U256,
    balance: U256,
) -> Result<String, TransferError> {
    let provider = &gateway.config.provider;

    let base_fee = provider
        .get_block(BlockNumber::Latest)
        .await?
        .and_then(|b| b.base_fee_per_gas)
        .ok_or(TransferError::BaseFee)?;

    match estimate_eip1559_fees_with_retry(
        provider,
        gateway.config.eip1559_estimation_retry_max,
        gateway.config.eip1559_estimation_retry_max,
    )
    .await
    {
        Ok((estimated_max_fee, estimated_priority_fee)) => {
            let max_fee_per_gas =
                std::cmp::max(estimated_max_fee, base_fee + estimated_priority_fee);

            let mut transaction = Eip1559TransactionRequest::new()
                .from(invoice.to)
                .to(gateway.config.treasury_address)
                .nonce(0)
                .chain_id(chain_id.as_u64())
                .max_fee_per_gas(max_fee_per_gas)
                .max_priority_fee_per_gas(estimated_priority_fee)
                .value(U256::zero());

            let gas_estimate =
                estimate_gas_on_transaction(provider, transaction.clone().into()).await?;
            let max_total_fee = max_fee_per_gas.mul(gas_estimate);

            transaction = transaction
                .gas(gas_estimate)
                .value(balance.saturating_sub(max_total_fee));

            transmit_transaction(signer, transaction.into(), chain_id, gateway).await
        }
        Err(e) => {
            log::error!("Could not estimate fees: {}", e);
            Err(TransferError::SendTransaction)
        }
    }
}

/// Transfers gas from a paid invoice to a specified treasury address
pub async fn transfer_gas_to_treasury(
    gateway: PaymentGateway,
    invoice: &Invoice,
) -> Result<String, TransferError> {
    let signer = LocalWallet::from_bytes(&invoice.wallet).unwrap();
    let chain_id = get_chain_id(gateway.config.provider.clone()).await?;
    let gas_price = get_gas_price(gateway.config.provider.clone()).await?;
    let balance = get_native_balance(&gateway.config.provider, &invoice.to).await?;

    match gateway.config.transaction_type {
        TransactionType::Legacy => {
            transfer_gas_to_treasury_legacy(gateway, invoice, signer, chain_id, gas_price, balance)
                .await
        }
        TransactionType::Eip1559 => {
            transfer_gas_to_treasury_eip1559(gateway, invoice, signer, chain_id, balance).await
        }
    }
}
