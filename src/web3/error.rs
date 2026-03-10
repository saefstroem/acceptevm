use thiserror::Error;

#[derive(Error, Debug)]
pub enum TransferError {
    #[error("Invalid wallet key: {0}")]
    InvalidWalletKey(#[from] std::array::TryFromSliceError),
    #[error("Invalid signer key: {0}")]
    InvalidSignerKey(#[from] k256::ecdsa::Error),
    #[error("Invalid RPC URL: {0}")]
    InvalidRpcUrl(#[from] url::ParseError),
    #[error("Insufficient balance for transfer")]
    InsufficientBalance,
    #[error("Transport error: {0}")]
    Transport(#[from] alloy::transports::TransportError),
    #[error("Transaction not confirmed: {0}")]
    PendingTransaction(#[from] alloy::providers::PendingTransactionError),
    #[error("Invalid transaction hash")]
    InvalidTxHash,
}
