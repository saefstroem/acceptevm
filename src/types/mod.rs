mod errors;
use std::ops::{Deref, DerefMut};

use self::errors::SerializableError;
use alloy::{
    primitives::{B256, U256},
    rpc::types::eth::TransactionReceipt,
};
use serde::{Deserialize, Serialize};
use zeroize::ZeroizeOnDrop;
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
    /// The address of the ERC20 token
    pub token_address: Option<String>,
}

#[derive(ZeroizeOnDrop, Clone, Deserialize, Serialize)]
pub struct ZeroizedB256 {
    pub inner: B256,
}

// To automatically dereference into B256 type for restoring wallet
impl Deref for ZeroizedB256 {
    type Target = B256;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl DerefMut for ZeroizedB256 {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.inner
    }
}

#[derive(Clone, Deserialize, Serialize)]
pub struct Invoice {
    /// Recipient address
    pub to: String,
    /// Recipient instance
    pub wallet: ZeroizedB256,
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
