pub mod error;
mod hash;
mod result;

use std::{
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
    time::{SystemTime, UNIX_EPOCH},
};

use ahash::AHashMap;
use alloy::signers::local::PrivateKeySigner;
use tokio::sync::mpsc::UnboundedSender;
use tokio::sync::RwLock;

pub use alloy::primitives::{Address, U256};

use crate::{
    invoice::{self, Invoice},
    web3::invoice_poller::poll_payments,
};

use self::{error::GatewayError, hash::hash_now};

use result::Result;

/// Wei is a type alias for `U256`, the smallest unit of the native currency.
pub type Wei = U256;

/// Retrieve the current unix time in seconds.
pub fn get_unix_time_seconds() -> u64 {
    let now = SystemTime::now();
    match now.duration_since(UNIX_EPOCH) {
        Ok(duration) => duration.as_secs(),
        Err(_) => 0,
    }
}

/// ## AcceptEVM
///
/// The payment gateway is designed to be ran on the main thread, all of
/// the functions are non-blocking asynchronous functions. The underlying polling
/// mechanism is offloaded using `tokio::spawn`. All invoices are stored
/// in-memory using an AHashMap. Therefore, it is your responsibility to
/// implement persistency for the invoices if you deem that this is required.
///
/// The payment gateway creates addresses and waits for payments to be made to these addresses.
/// When a deposit is made to the address, the gateway will check the balance and if the balance is
/// greater than or equal to the amount specified in the invoice, the gateway will consider the invoice
/// paid and will transfer the funds to the treasury address. However, due to the uncertainty of the blockchain
/// this transfer could fail. It is therefore important to check if the hash is present in the invoice when
/// receiving the invoice from the receiver.
///
/// If the hash is present, the invoice was successfully transferred to the treasury. If the hash is not present,
/// the invoice was not transferred to the treasury, and you should handle this case accordingly. The invoice will
/// always contain the wallet bytes that were used to create the invoice. You can use these bytes to recover the
/// funds using `alloy::signers::local::PrivateKeySigner::from_bytes()`. It is therefore important to store this
/// wallet in a safe location for either programmatic or manual recovery.
///
/// Example:
/// ```rust
/// use acceptevm::gateway::{PaymentGateway, PaymentGatewayConfiguration, Address, Wei};
///
/// #[tokio::main]
/// async fn main() -> std::result::Result<(), Box<dyn std::error::Error>> {
///     let (sender, _receiver) = tokio::sync::mpsc::unbounded_channel();
///     let gateway = PaymentGateway::new(
///         PaymentGatewayConfiguration {
///             rpc_urls: vec![
///                 "https://bsc-dataseed1.binance.org/".to_string(),
///                 "https://bsc-dataseed2.binance.org/".to_string(),
///             ],
///             treasury_address: "0xdac17f958d2ee523a2206206994597c13d831ec7"
///                 .parse::<Address>()?,
///             min_confirmations: 10,
///             sender,
///             poller_delay_seconds: 10,
///             receipt_timeout_seconds: 60,
///         },
///     )?;
///
///     // Add new invoice
///     let (invoice_id, invoice) = gateway
///         .new_invoice(Wei::from(100), b"Invoice details".to_vec(), 3600)
///         .await?;
///
///     // Get the invoice from the gateway
///     let invoice = gateway.get_invoice(&invoice_id).await?;
///
///     gateway.poll_payments().await;
///     // Continuously receive the paid invoices via the _receiver.
///     Ok(())
/// }
/// ```
#[derive(Clone)]
pub struct PaymentGateway {
    pub config: PaymentGatewayConfiguration,
    pub invoices: Arc<RwLock<AHashMap<String, Invoice>>>,
    rpc_index: Arc<AtomicUsize>,
}

/// ## PaymentGatewayConfiguration
///
/// - `rpc_urls`: a list of RPC provider URLs. Requests are distributed across them using round-robin.
/// - `treasury_address`: the address of the treasury for all paid invoices.
/// - `min_confirmations`: the minimum amount of confirmations required before considering a transaction confirmed.
/// - `sender`: an `UnboundedSender` from a tokio mpsc channel to receive paid invoices.
/// - `poller_delay_seconds`: how long to wait between checking invoices. This prevents potential rate limits.
/// - `receipt_timeout_seconds`: how long to wait for a transaction receipt before timing out.
#[derive(Clone)]
pub struct PaymentGatewayConfiguration {
    pub rpc_urls: Vec<String>,
    pub treasury_address: Address,
    pub poller_delay_seconds: u64,
    pub sender: UnboundedSender<(String, Invoice)>,
    pub min_confirmations: u64,
    pub receipt_timeout_seconds: u64,
}

impl PaymentGateway {
    /// Creates a new payment gateway.
    ///
    /// Returns an error if `rpc_urls` is empty.
    ///
    /// Example:
    /// ```rust
    /// use acceptevm::gateway::{PaymentGateway, PaymentGatewayConfiguration, Address};
    ///
    /// # fn main() -> std::result::Result<(), Box<dyn std::error::Error>> {
    /// let (sender, _receiver) = tokio::sync::mpsc::unbounded_channel();
    /// let gateway = PaymentGateway::new(
    ///     PaymentGatewayConfiguration {
    ///         rpc_urls: vec!["https://bsc-dataseed1.binance.org/".to_string()],
    ///         treasury_address: "0xdac17f958d2ee523a2206206994597c13d831ec7"
    ///             .parse::<Address>()?,
    ///         min_confirmations: 10,
    ///         sender,
    ///         poller_delay_seconds: 10,
    ///         receipt_timeout_seconds: 60,
    ///     },
    /// )?;
    /// # Ok(())
    /// # }
    /// ```
    pub fn new(configuration: PaymentGatewayConfiguration) -> Result<PaymentGateway> {
        if configuration.rpc_urls.is_empty() {
            return Err(GatewayError::NoRpcUrls);
        }
        Ok(PaymentGateway {
            config: configuration,
            invoices: Arc::new(RwLock::new(AHashMap::new())),
            rpc_index: Arc::new(AtomicUsize::new(0)),
        })
    }

    /// Returns the next RPC URL using round-robin selection.
    pub fn next_rpc_url(&self) -> &str {
        let idx = self.rpc_index.fetch_add(1, Ordering::Relaxed) % self.config.rpc_urls.len();
        &self.config.rpc_urls[idx]
    }

    /// Retrieves all invoices as a list of `(id, invoice)` tuples.
    /// The key is a SHA256 hash of the recipient address.
    pub async fn get_all_invoices(&self) -> Result<Vec<(String, Invoice)>> {
        let invoices = self
            .invoices
            .read()
            .await
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();
        Ok(invoices)
    }

    /// Retrieve an invoice from the payment gateway by its ID.
    pub async fn get_invoice(&self, key: &str) -> Result<Invoice> {
        self.invoices
            .read()
            .await
            .get(key)
            .cloned()
            .ok_or(GatewayError::NotFound)
    }

    /// Spawns an asynchronous task that checks all the pending invoices
    /// for this gateway.
    pub async fn poll_payments(&self) {
        let gateway = self.clone();
        tokio::spawn(poll_payments(gateway));
    }

    /// Creates a new invoice for this gateway.
    ///
    /// When this invoice is paid it will be sent through the sender channel.
    ///
    /// The `amount` parameter is in the smallest unit of the currency (wei for ETH).
    /// The `message` parameter accepts an array of bytes for arbitrary data.
    /// The `expires_in_seconds` parameter sets how long the invoice is valid.
    pub async fn new_invoice(
        &self,
        amount: Wei,
        message: Vec<u8>,
        expires_in_seconds: u64,
    ) -> Result<(String, Invoice)> {
        let signer = PrivateKeySigner::random();
        let invoice = Invoice {
            to: signer.address(),
            wallet: invoice::ZeroizedVec {
                inner: signer.credential().to_bytes().to_vec(),
            },
            amount,
            message,
            paid_at_timestamp: 0,
            expires: get_unix_time_seconds() + expires_in_seconds,
            hash: None,
            nonce: None,
        };

        let invoice_id = hash_now(signer.address().0.as_slice());
        self.invoices
            .write()
            .await
            .insert(invoice_id.clone(), invoice.clone());
        Ok((invoice_id, invoice))
    }
}
