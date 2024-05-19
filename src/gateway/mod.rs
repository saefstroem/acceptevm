pub mod errors;
mod hash;
use std::{
    future::Future,
    pin::Pin,
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};

use async_std::channel::Sender;
use crossbeam_skiplist::SkipMap;
use ethers::signers::Signer;

pub type Provider<T> = ethers::providers::Provider<T>;
pub type Http = ethers::providers::Http;
pub type Address = ethers::types::Address;
pub type U256 = ethers::types::U256;
pub type Units = ethers::utils::Units;
pub type LocalWallet = ethers::signers::LocalWallet;
pub type SecretKey = ethers::core::k256::SecretKey;
pub use ethers::utils::hex;

use crate::{
    invoice::{self, Invoice},
    web3::poller::poll_payments,
};

use self::{errors::GatewayError, hash::hash_now};

/// Retrieve the current unix time in nanoseconds
pub fn get_unix_time_millis() -> u128 {
    let now = SystemTime::now();
    let duration = now.duration_since(UNIX_EPOCH).unwrap_or_default();
    duration.as_millis()
}
/// Retrieve the current unix time in nanoseconds
pub fn get_unix_time_seconds() -> u64 {
    let now = SystemTime::now();
    let duration = now.duration_since(UNIX_EPOCH).unwrap_or_default();
    duration.as_secs()
}

/// ## AcceptEVM
///
/// The payment gateway is designed to be ran on the main thread, all of
/// the functions are non-blocking asynchronous functions. The underlying polling
/// mechanism is offloaded using `tokio::spawn`. All invoices are stored
/// in-memory for now using a SkipMap. Therefore, it is your responsibility to
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
/// always contain the wallet that was used to create the invoice. This wallet is a LocalWallet from the ethers crate which
/// you can use to recover the funds. It is therefore important to store this wallet in a safe location for either programmatic
/// or manual recovery. The `to_string()` method of the invoice will convert all invoice data into a readable format
/// that can be stored in a CSV file or any other format that you prefer. The wallet will be transpiled into a private key
/// compatible with wallets like Metamask and TrustWallet.
///
///
/// Example:
/// ```rust
/// use acceptevm::gateway::{Provider,PaymentGateway,TransactionType,PaymentGatewayConfiguration, Reflector,Address};
/// use async_std::channel::unbounded;
/// use acceptevm::gateway::Wei;
///
/// #[tokio::main]
/// async fn main(){
///     let (sender, receiver) = unbounded();
///     let reflector = Reflector::Sender(sender);
///     let transaction_type=TransactionType::Eip1559;
///     let provider = Provider::try_from("https://bsc-dataseed1.binance.org/").expect("Invalid RPC URL");
///     let gateway = PaymentGateway::new(
///         PaymentGatewayConfiguration{
///             provider,
///             treasury_address: "0xdac17f958d2ee523a2206206994597c13d831ec7".parse::<Address>().unwrap(),
///             min_confirmations: 10,
///             reflector,
///             poller_delay_seconds: 10,
///             transaction_type,
///             eip1559_estimation_retry_max: 3,
///             eip1559_estimation_retry_delay_seconds: 10,   
///         }
///      );
///     
///     // Add new invoice and serialize string data with bincode
///     let (invoice_id, invoice) = gateway.new_invoice(
///         Wei::from(100),
///         None,
///         bincode::serialize("Invoice details").expect("Could not serialize invoice details"),
///         3600
///     ).await.unwrap();
///
///     // Get the invoice from the gateway
///    let invoice = gateway.get_invoice(&invoice_id).await.unwrap();
///     
///     gateway.poll_payments().await;
///     // Continously receive the paid invoices via the receiver.
///     // You can implement your own logic here.
///     // Example: (note the return type of the receiver)
///     /* while let Ok(invoice:(String,Invoice)) = receiver.recv().await {
///        println!("Received invoice: {:?}", invoice);
///        break;
///     }
///     */
/// }
/// ```
///
///
#[derive(Clone)]
pub struct PaymentGateway {
    pub config: PaymentGatewayConfiguration,
    pub invoices: Arc<SkipMap<String, Invoice>>,
}

#[derive(Clone)]
pub enum TransactionType {
    Legacy,
    Eip1559,
}

/// ## PaymentGatewayConfiguration
/// The configuration struct contains the following fields:
/// - `provider`: the provider for the EVM network. This is used to communicate with the EVM network.
/// - `treasury_address`: the address of the treasury for all paid invoices, on this EVM network.
/// - `min_confirmations`: the minimum amount of confirmations required before considering an invoice paid.
/// - `reflector`: The reflector is an enum that allows you to receive the paid invoices.
/// At the moment, the only reflector available is the `Sender` from the async-std channel.
/// This means that you will need to create a channel and pass the sender as the reflector.
/// - `poller_delay_seconds`: how long to wait before checking the next invoice in milliseconds.
/// This is used to prevent potential rate limits from the node.
/// - `transaction_type`: the type of transaction to use. At the moment, the only two options are `Legacy` and `Eip1559`.
/// - `eip1559_estimation_retry_max`: the maximum amount of retries for the EIP1559 estimation. The latest block data
/// is used to estimate the gas prices for the transaction. If the block is empty, the gateway will retry until the
/// maximum amount of retries is reached. Take this into consideration when deploying the gateway on an EVM network.
/// - `eip1559_estimation_retry_delay_seconds`: the delay between each retry in seconds.
#[derive(Clone)]
pub struct PaymentGatewayConfiguration {
    pub provider: Provider<Http>,
    pub treasury_address: Address,
    pub poller_delay_seconds: u64,
    pub reflector: Reflector,
    pub min_confirmations: usize,
    pub transaction_type: TransactionType,
    pub eip1559_estimation_retry_max: u64,
    pub eip1559_estimation_retry_delay_seconds: u64,
}

/// ## Reflector
/// The reflector allows your payment gateway to be used in a more flexible way.
///
/// In its current state you can pass a Sender from an unbound async-std channel
/// which you can create by doing:
/// ```rust
/// use async_std::channel::unbounded;
/// use acceptevm::gateway::Reflector;
///
/// let (sender, receiver) = unbounded();
///
/// let reflector=Reflector::Sender(sender);
/// ```
///
/// You may clone the receiver as many times as you want but do not use the sender
/// for anything other than passing it to the try_new() method.
#[derive(Clone)]
pub enum Reflector {
    /// A sender from async-std
    Sender(Sender<(String, Invoice)>),
}

// Type alias for the underlying Web3 type.
pub type Wei = U256;

// Type alias for the invoice callback function
pub type AsyncCallback =
    Arc<dyn Fn(Invoice) -> Pin<Box<dyn Future<Output = ()> + Send>> + Send + Sync>;

impl PaymentGateway {
    /// ## Creates a new payment gateway.
    ///
    /// To minimize the amount of arguments, the configuration is passed as a struct.
    ///
    /// The configuration struct contains the following fields:
    /// - `provider`: the provider for the EVM network. This is used to communicate with the EVM network.
    /// - `treasury_address`: the address of the treasury for all paid invoices, on this EVM network.
    /// - `min_confirmations`: the minimum amount of confirmations required before considering an invoice paid.
    /// - `reflector`: The reflector is an enum that allows you to receive the paid invoices.
    /// At the moment, the only reflector available is the `Sender` from the async-std channel.
    /// This means that you will need to create a channel and pass the sender as the reflector.
    /// - `invoice_delay_seconds`: how long to wait before checking the next invoice in milliseconds.
    /// This is used to prevent potential rate limits from the node.
    /// - `transaction_type`: the type of transaction to use. At the moment, the only two options are `Legacy` and `Eip1559`.
    /// - `eip1559_estimation_retry_max`: the maximum amount of retries for the EIP1559 estimation. The latest block data
    /// is used to estimate the gas prices for the transaction. If the block is empty, the gateway will retry until the
    /// maximum amount of retries is reached. Take this into consideration when deploying the gateway on an EVM network.
    /// - `eip1559_estimation_retry_delay_seconds`: the delay between each retry in seconds.
    ///
    /// Example:
    /// ```rust
    /// use acceptevm::gateway::{Provider,PaymentGateway,TransactionType,PaymentGatewayConfiguration, Reflector,Address};
    /// use async_std::channel::unbounded;
    ///
    /// let (sender, receiver) = unbounded();
    /// let reflector = Reflector::Sender(sender);
    /// let transaction_type=TransactionType::Eip1559;
    /// let provider = Provider::try_from("https://bsc-dataseed1.binance.org/").expect("Invalid RPC URL");
    /// let gateway = PaymentGateway::new(
    ///     PaymentGatewayConfiguration{
    ///         provider,
    ///         treasury_address: "0xdac17f958d2ee523a2206206994597c13d831ec7".parse::<Address>().unwrap(),
    ///         min_confirmations: 10,
    ///         reflector,
    ///         poller_delay_seconds: 10,
    ///         transaction_type,
    ///         eip1559_estimation_retry_max: 3,
    ///         eip1559_estimation_retry_delay_seconds: 10,   
    ///     }
    ///  );
    /// ```
    pub fn new(configuration: PaymentGatewayConfiguration) -> PaymentGateway {
        let map: SkipMap<String, Invoice> = SkipMap::new();
        PaymentGateway {
            config: configuration,
            invoices: Arc::new(map),
        }
    }

    /// Retrieves all invoices in the form of a tuple: String,Invoice
    /// where the first element is the key that was used in the database
    /// and the second part is the invoice. The key is a SHA256 hash of the
    /// creation timestamp and the recipient address.
    pub async fn get_all_invoices(&self) -> Result<Vec<(String, Invoice)>, GatewayError> {
        let invoices = self
            .invoices
            .iter()
            .map(|entry| (entry.key().clone(), entry.value().clone()))
            .collect();
        Ok(invoices)
    }

    /// Retrieve an invoice from the payment gateway
    pub async fn get_invoice(&self, key: &str) -> Result<Invoice, GatewayError> {
        let invoices = self.invoices.get(key);
        match invoices {
            Some(invoice) => Ok(invoice.value().clone()),
            None => Err(GatewayError::NotFound),
        }
    }

    /// Spawns an asynchronous task that checks all the pending invoices
    /// for this gateway.
    pub async fn poll_payments(&self) {
        let gateway = self.clone();
        tokio::spawn(poll_payments(gateway));
    }

    /// Creates a new invoice for this gateway. When this invoice is paid
    /// it will be passed into the callback function of the callback function
    /// that you provided when instantiating the gateway.
    ///
    /// **Wei** a type alias for `web3::types::U256`
    /// The message parameter accepts an array of bytes. It is suggested
    /// to use `bincode` for serialization. The serialization was not implemented internally
    /// because one might want to serialize structs and use them as part of the message field.
    pub async fn new_invoice(
        &self,
        amount: Wei,
        token_address: Option<Address>,
        message: Vec<u8>,
        expires_in_seconds: u64,
    ) -> Result<(String, Invoice), GatewayError> {
        // Panic if token address is set
        if token_address.is_some() {
            panic!("Token address is not supported yet");
        }

        // Generate random wallet
        let signer = LocalWallet::new(&mut ethers::core::rand::thread_rng());
        let invoice = Invoice {
            to: signer.address(),
            wallet: invoice::ZeroizedVec {
                inner: signer.signer().to_bytes().to_vec(),
            },
            amount,
            token_address,
            message,
            paid_at_timestamp: 0,
            expires: get_unix_time_seconds() + expires_in_seconds,
            hash: None,
        };

        // Create collision-safe key for the map
        let seed = format!("{}{}", signer.address(), get_unix_time_millis());
        let invoice_id = hash_now(seed);
        // Save the invoice in db.
        self.invoices.insert(invoice_id.clone(), invoice.clone());
        Ok((invoice_id, invoice))
    }
}
