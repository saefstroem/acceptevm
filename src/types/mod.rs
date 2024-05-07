mod errors;
use serde::{Deserialize, Serialize};
use web3::types::U256;
use self::errors::SerializableError;
pub trait Serializable {
    fn to_bin(&self) -> Result<Vec<u8>,Box<bincode::ErrorKind>>;
    fn from_bin(data: Vec<u8>) -> Result<Self, SerializableError> where Self: Sized;
}

/// Describes the structure of a payment method in
/// a gateway
#[derive(Clone, Deserialize,Serialize)]
pub struct PaymentMethod {
    /// Whether or not the method uses native gas token
    pub is_native: bool,
    /// The address of the ERC20 token
    pub token_address: Option<String>,
}

#[derive(Clone, Deserialize,Serialize)]
pub struct Invoice {
    pub to:String,
    pub amount: U256,
    pub method: PaymentMethod,
    pub message: Vec<u8>,
    pub paid_at_timestamp: u64,
    pub expires:u64
}

impl Serializable for Invoice {
    fn to_bin(&self) -> Result<Vec<u8>,Box<bincode::ErrorKind>> {
        bincode::serialize(&self)
     }
     fn from_bin(data: Vec<u8>) -> Result<Self, SerializableError> {
        bincode::deserialize(&data).map_err(|_| SerializableError::Deserialize)
     }
 }