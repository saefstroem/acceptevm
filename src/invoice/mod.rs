use alloy::primitives::{Address, U256};
use serde::{Deserialize, Serialize};
use std::ops::{Deref, DerefMut};
use zeroize::ZeroizeOnDrop;

/// ## DANGER: Private Key Data is contained in this struct
/// Zeroed memory on drop
#[derive(ZeroizeOnDrop, Clone, Deserialize, Serialize, Debug)]
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

#[derive(Clone, Deserialize, Serialize, Debug)]
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
    /// Transaction hash of the treasury transfer
    pub hash: Option<String>,
    /// Nonce used for the treasury transfer (for replacement txs)
    pub nonce: Option<u64>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloy::primitives::U256;

    fn make_vec(bytes: Vec<u8>) -> ZeroizedVec {
        ZeroizedVec { inner: bytes }
    }

    #[test]
    fn zeroized_vec_deref_gives_inner_slice() {
        let v = make_vec(vec![1, 2, 3]);
        assert_eq!(&*v, &[1u8, 2, 3]);
    }

    #[test]
    fn zeroized_vec_deref_mut_allows_mutation() {
        let mut v = make_vec(vec![10, 20]);
        (*v)[0] = 99;
        assert_eq!(v.inner[0], 99);
    }

    #[test]
    fn zeroized_vec_clone_is_independent() {
        let original = make_vec(vec![1, 2, 3]);
        let mut clone = original.clone();
        clone.inner[0] = 255;
        assert_eq!(original.inner[0], 1, "original must not be affected by clone mutation");
    }

    #[test]
    fn invoice_clone_is_structurally_equal() {
        let inv = Invoice {
            to: Address::repeat_byte(0xAB),
            wallet: make_vec(vec![0u8; 32]),
            amount: U256::from(42u64),
            message: b"hello".to_vec(),
            expires: 9999,
            paid_at_timestamp: 0,
            hash: None,
            nonce: None,
        };
        let clone = inv.clone();
        assert_eq!(inv.to, clone.to);
        assert_eq!(inv.amount, clone.amount);
        assert_eq!(inv.message, clone.message);
        assert_eq!(inv.expires, clone.expires);
    }

    #[test]
    fn invoice_default_state_fields() {
        let inv = Invoice {
            to: Address::ZERO,
            wallet: make_vec(vec![]),
            amount: U256::ZERO,
            message: vec![],
            expires: 0,
            paid_at_timestamp: 0,
            hash: None,
            nonce: None,
        };
        assert!(inv.hash.is_none());
        assert!(inv.nonce.is_none());
        assert_eq!(inv.paid_at_timestamp, 0);
    }
}
