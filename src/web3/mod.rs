mod erc20;
pub mod poller;
mod transfers;

use ethers::types::BlockNumber::Latest;
use ethers::{
    providers::{Http, Middleware, Provider, ProviderError},
    types::{Address, BlockId, BlockNumber, U256},
};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum TransferError {
    #[error("Could not get base fee")]
    BaseFee,
    #[error("Could not transmit transaction")]
    SendTransaction,
    #[error("Transaction not confirmed")]
    TransactionNotConfirmed,
    #[error("Ethers error: {0}")]
    EthersError(#[from] ProviderError),
}
#[derive(Error, Debug)]
pub enum FeeEstimationError {
    #[error("No transactions in block")]
    NoTransactionsInBlock,
    #[error("No base fee in block")]
    NoBaseFeeInBlock,
    #[error("Ethers error: {0}")]
    EthersError(#[from] ProviderError),
}

/// Estimates EIP-1559 transaction fees (max fee per gas and max priority fee per gas) with retries
pub async fn estimate_eip1559_fees_with_retry(
    provider: &Provider<Http>,
    max_retries: u64,
    delay_seconds_in_between_retries: u64,
) -> Result<(U256, U256), FeeEstimationError> {
    let mut retries = 0;

    loop {
        match estimate_eip1559_fees(provider).await {
            Ok(fees) => return Ok(fees),
            Err(FeeEstimationError::NoTransactionsInBlock)
            | Err(FeeEstimationError::NoBaseFeeInBlock) => {
                if retries >= max_retries {
                    return Err(FeeEstimationError::NoTransactionsInBlock);
                }
                retries += 1;
            }
            Err(e) => return Err(e),
        }
        // Sleep
        tokio::time::sleep(tokio::time::Duration::from_secs(
            delay_seconds_in_between_retries,
        ))
        .await;
    }
}
/// Estimates EIP-1559 transaction fees (max fee per gas and max priority fee per gas)
async fn estimate_eip1559_fees(
    provider: &Provider<Http>,
) -> Result<(U256, U256), FeeEstimationError> {
    let block = provider
        .get_block_with_txs(BlockId::Number(BlockNumber::Latest))
        .await?
        .ok_or(FeeEstimationError::NoTransactionsInBlock)?;

    let base_fee = block
        .base_fee_per_gas
        .ok_or(FeeEstimationError::NoBaseFeeInBlock)?;

    let mut total_max_fee = U256::zero();
    let mut total_priority_fee = U256::zero();
    let count = block.transactions.len() as u64;

    if count == 0 {
        return Err(FeeEstimationError::NoTransactionsInBlock);
    }

    for tx in block.transactions {
        if let Some(max_fee_per_gas) = tx.max_fee_per_gas {
            total_max_fee += max_fee_per_gas;
            // Calculate priority fee as max_fee - base_fee
            total_priority_fee += max_fee_per_gas.saturating_sub(base_fee);
        }
    }

    let average_max_fee = total_max_fee / U256::from(count);
    let average_priority_fee = total_priority_fee / U256::from(count);

    Ok((average_max_fee, average_priority_fee))
}

/// Retrieves the gas token balance of the specified address on the specified web3 instance
pub async fn get_native_balance(
    provider: &Provider<Http>,
    address: &Address,
) -> Result<U256, TransferError> {
    Ok(provider
        .get_balance(*address, Some(BlockId::Number(Latest)))
        .await?)
}

// Retrieves the chain id from the provider.
pub async fn get_chain_id(provider: Provider<Http>) -> Result<U256, TransferError> {
    Ok(provider.get_chainid().await?)
}

/// Retrieves the current gas price from a provider
pub async fn get_gas_price(provider: Provider<Http>) -> Result<U256, TransferError> {
    Ok(provider.get_gas_price().await?)
}
