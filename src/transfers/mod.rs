pub mod errors;
pub mod gas_transfers;

use alloy::{
    providers::{Provider, RootProvider},
    transports::http::Http,
};
use reqwest::Client;

use self::errors::TransferError;

// Retrieves the chain id from the provider.
async fn get_chain_id(provider: RootProvider<Http<Client>>) -> Result<u64, TransferError> {
    match provider.get_chain_id().await {
        Ok(chain_id) => Ok(chain_id),
        Err(error) => {
            log::error!("Could not get chain id: {}", error);
            Err(TransferError::ChainId)
        }
    }
}

/// Retrieves the current gas price from a provider
async fn get_gas_price(provider: RootProvider<Http<Client>>) -> Result<u128, TransferError> {
    match provider.get_gas_price().await {
        Ok(gas_price) => Ok(gas_price),
        Err(error) => {
            log::error!(
                "Could not get gas price (maybe chain uses EIP-1559?): {}",
                error
            );
            Err(TransferError::ChainId)
        }
    }
}
