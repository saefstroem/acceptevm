use std::ops::{Deref, DerefMut};
use alloy::primitives::{Address, U256};
use serde::{Deserialize, Serialize};
use zeroize::ZeroizeOnDrop;

/// ## DANGER: Private Key Data is contained in this struct
/// Zeroed memory on drop
#[derive(ZeroizeOnDrop, Clone, Deserialize, Serialize,Debug)]
pub struct ZeroizedVec {
    pub inner: Vec<u8>,
}

// To automatically dereference into Vec<u8> type for restoring wallet
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

#[derive(Clone, Deserialize, Serialize,Debug)]
pub struct Invoice {
    /// Recipient address
    pub to: Address,
    /// Contains the keys to restore the wallet
    pub wallet: ZeroizedVec,
    /// Amount requested
    pub amount: U256,
    /// Arbitrary message attached to the invoice
    pub message: Vec<u8>,
    /// Invoice expiry time
    pub expires: u64,
    /// Timestamp at which the invoice was paid
    pub paid_at_timestamp: u64,
    /// Transaction hash of the payment transaction
    pub hash: Option<String>,
}
