use std::ops::{Deref, DerefMut};


use ethers::types::{Address, TransactionReceipt, U256};
use serde::{Deserialize, Serialize};
use zeroize::ZeroizeOnDrop;

/// Describes the structure of a payment method in
/// a gateway
#[derive(Clone, Deserialize, Serialize)]
pub struct PaymentMethod {
    /// The address of the ERC20 token
    pub token_address: Option<Address>,
}

#[derive(ZeroizeOnDrop, Clone, Deserialize, Serialize)]
pub struct ZeroizedVec {
    pub inner: Vec<u8>,
}

// To automatically dereference into Vec type for restoring wallet
impl Deref for ZeroizedVec {
    type Target = Vec<u8>;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl DerefMut for ZeroizedVec {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.inner
    }
}

#[derive(Clone, Deserialize, Serialize)]
pub struct Invoice {
    /// Recipient address
    pub to: Address,
    /// Recipient instance
    pub wallet: ZeroizedVec,
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

