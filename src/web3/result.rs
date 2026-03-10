use super::error::TransferError;

pub type Result<T> = std::result::Result<T, TransferError>;
