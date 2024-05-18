use std::ops::{Deref, DerefMut};

use ethers::{core::k256::SecretKey, signers::LocalWallet, types::{Address, U256}};
use serde::{Deserialize, Serialize};
use zeroize::ZeroizeOnDrop;
use ethers::utils::hex;
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
    pub hash: Option<String>,
}

impl Invoice {
    // A to_string method for the Invoice struct that is used for output to csv files
    pub fn to_string(&self) -> String {
        let wallet = LocalWallet::from_bytes(&self.wallet).expect("Invalid key");

        // Extract the private key as a scalar
        let signer = wallet.signer();


        // Convert the scalar into a Secp256k1 secret key
        let secret_key = SecretKey::try_from(signer.as_nonzero_scalar()).expect("Failed to convert to secret key");

        // Encode secret key to bytes
        let private_key_bytes = secret_key.to_bytes();

        // Convert to hex string for display/import.
        let private_key_string = hex::encode(private_key_bytes);

        format!(
            "{},{},{},{},{},{},{},{}",
            self.to,
            private_key_string,
            self.amount,
            self.method.token_address.unwrap_or_default(),
            self.message
                .iter()
                .map(|x| format!("{:02x}", x))
                .collect::<String>(),
            self.paid_at_timestamp,
            self.expires,
            self.hash.as_ref().unwrap_or(&"".to_string())
        )
    }
}
