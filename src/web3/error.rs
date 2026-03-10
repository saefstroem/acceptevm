use thiserror::Error;

#[derive(Error, Debug)]
pub enum TransferError {
    #[error("Could not transmit transaction")]
    SendTransaction,
    #[error("Transaction not confirmed")]
    TransactionNotConfirmed,
    #[error("Alloy transport error: {0}")]
    Transport(#[from] alloy::transports::TransportError),
}
