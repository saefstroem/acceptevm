mod errors;
use self::errors::SerializableError;
use alloy::{
    primitives::{B256, U256},
    rpc::types::eth::TransactionReceipt,
};
use serde::{Deserialize, Serialize};
pub trait Serializable {
    fn to_bin(&self) -> Result<Vec<u8>, Box<bincode::ErrorKind>>;
    fn from_bin(data: Vec<u8>) -> Result<Self, SerializableError>
    where
        Self: Sized;
}

/// Describes the structure of a payment method in
/// a gateway
#[derive(Clone, Deserialize, Serialize)]
pub struct PaymentMethod {
    /// Whether or not the method uses native gas token
    pub is_native: bool,
    /// The address of the ERC20 token
    pub token_address: Option<String>,
}

#[derive(Clone, Deserialize, Serialize)]
pub struct Invoice {
    /// Recipient address
    pub to: String,
    /// Recipient instance
    pub wallet: B256,
    /// Amount requested
    pub amount: U256,
    /// Method used for payment
    pub method: PaymentMethod,
    /// Arbitrary message attached to the invoice
    pub message: Vec<u8>,
    /// Timestamp at which the invoice was paid
    pub paid_at_timestamp: u64,
    /// Invoice expiry time
    pub expires: u64,
    pub receipt: Option<TransactionReceipt>,
}

impl Serializable for Invoice {
    /// Serializes invoice to bytes
    fn to_bin(&self) -> Result<Vec<u8>, Box<bincode::ErrorKind>> {
        bincode::serialize(&self)
    }

    /// Deserializes invoice from bytes
    fn from_bin(data: Vec<u8>) -> Result<Self, SerializableError> {
        bincode::deserialize(&data).map_err(|_| SerializableError::Deserialize)
    }
}
