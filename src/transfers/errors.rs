use thiserror::Error;

#[derive(Error, Debug, PartialEq)]
pub enum TransferError {
    #[error("Could not get chain id")]
    ChainId,
    #[error("Could not transmit transaction")]
    SendTransaction,
    #[error("Could not create transaction")]
    CreateTransaction,
}
