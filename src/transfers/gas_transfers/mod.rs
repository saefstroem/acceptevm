use alloy::{
    consensus::TxEnvelope,
    network::{
        eip2718::Encodable2718, Ethereum, EthereumSigner, TransactionBuilder,
        TransactionBuilderError,
    },
    primitives::U256,
    providers::{Provider, RootProvider},
    rpc::types::eth::{TransactionReceipt, TransactionRequest},
    signers::{
        k256::ecdsa::SigningKey,
        wallet::{LocalWallet, Wallet},
    },
    transports::{http::Http, RpcError, TransportErrorKind},
};

use reqwest::Client;
use std::ops::Mul;

use crate::{
    gateway::{PaymentGateway, PaymentGatewayConfiguration},
    types::Invoice,
};

use super::{errors::TransferError, get_chain_id, get_gas_price};

/// Wrapper function for alloy's send transaction method to minimize
/// the number of nested match statements.
async fn send_transaction(
    transaction: Vec<u8>,
    provider: RootProvider<Http<Client>>,
) -> Result<TransactionReceipt, RpcError<TransportErrorKind>> {
    provider
        .send_raw_transaction(&transaction)
        .await?
        .get_receipt()
        .await
}

/// Crea
async fn create_transaction(
    gateway_config: PaymentGatewayConfiguration,
    invoice: Invoice,
    chain_id: u64,
    gas_price: u128,
    signer: Wallet<SigningKey>,
) -> Result<TxEnvelope, TransactionBuilderError<Ethereum>> {
    let ethereum_signer: EthereumSigner = signer.into();

    // Use specified gas limit or fallback
    let gas_limit = gateway_config.transfer_gas_limit.unwrap_or(21000);

    // Maximum cost of transaction
    let max_cost = gas_limit.mul(gas_price);

    // Estimated gas left after transfer
    let value = invoice.amount.saturating_sub(U256::from(max_cost));

    TransactionRequest::default()
        .from(invoice.to.parse().unwrap())
        .to(gateway_config.treasury_address)
        .with_nonce(0)
        .with_chain_id(chain_id)
        .with_gas_limit(gas_limit)
        .value(value)
        .with_gas_price(gas_price)
        .build(&ethereum_signer)
        .await
}

/// Transfers gas from a paid invoice to a specified treasury address
pub async fn transfer_gas_to_treasury(
    gateway: PaymentGateway,
    invoice: Invoice,
) -> Result<TransactionReceipt, TransferError> {
    let signer = LocalWallet::from_bytes(&invoice.clone().wallet).unwrap();
    let chain_id = get_chain_id(gateway.config.provider.clone()).await?;
    let gas_price = get_gas_price(gateway.config.provider.clone()).await?;

    // Create a transaction
    match create_transaction(gateway.config.clone(), invoice, chain_id, gas_price, signer).await {
        Ok(tx_envelope) => {
            let tx_encoded = tx_envelope.encoded_2718();
            // Send transaction and await receipt
            match send_transaction(tx_encoded, gateway.config.provider).await {
                Ok(receipt) => Ok(receipt),
                Err(error) => {
                    log::error!("Could not send transaction: {}", error);
                    Err(TransferError::SendTransaction)
                }
            }
        }
        Err(error) => {
            log::error!("Could not send transaction: {}", error);
            Err(TransferError::CreateTransaction)
        }
    }
}
