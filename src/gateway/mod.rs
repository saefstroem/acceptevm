mod hash;
use std::{future::Future, pin::Pin, str::FromStr, sync::Arc};

use alloy::{
    primitives::{Address, U256},
    providers::{ProviderBuilder, RootProvider},
    signers::wallet::LocalWallet,
    transports::http::Http,
};
use log::LevelFilter;
use log4rs::{
    append::file::FileAppender,
    config::{Appender, Root},
    encode::pattern::PatternEncoder,
    Config,
};
use reqwest::{Client, Url};
use sled::Tree;
use tokio::sync::Mutex;

use crate::{
    common::{get_unix_time_millis, get_unix_time_seconds, DatabaseError},
    db::{get, get_all, get_last, set},
    poller::poll_payments,
    types::{self, Invoice, PaymentMethod},
};

use self::hash::hash_now;

/// ## AcceptEVM
///
///
/// The payment gateway is designed to be ran on the main thread, all of
/// the functions are non-blocking asynchronous functions. The underlying polling
/// mechanism is offloaded using `tokio::spawn`.
#[derive(Clone)]
pub struct PaymentGateway {
    pub config: PaymentGatewayConfiguration,
    pub tree: Tree,
    pub name: String,
}

#[derive(Clone)]
pub struct PaymentGatewayConfiguration {
    pub provider: RootProvider<Http<Client>>,
    pub treasury_address: Address,
    pub invoice_delay_millis: u64,
    pub callback: AsyncCallback,
    pub transfer_gas_limit: Option<u128>,
}

// Type alias for the underlying Web3 type.
pub type Wei = U256;

// Type alias for the invoice callback function
pub type AsyncCallback =
    Arc<Mutex<dyn Fn(Invoice) -> Pin<Box<dyn Future<Output = ()> + Send>> + Send + Sync>>;

impl PaymentGateway {
    /// Creates a new payment gateway.
    ///
    /// - `rpc_url`: the HTTP Rpc url of the EVM network
    /// - `treasury_address`: the address of the treasury for all paid invoices, on this EVM network.
    /// - `invoice_delay_millis`: how long to wait before checking the next invoice in milliseconds.
    /// This is used to prevent potential rate limits from the node.
    /// - `callback`: an async function that is called when an invoice is paid.
    /// - `sled_path`: The path of the sled database where the pending invoices will
    /// be stored. In the event of a crash the invoices are saved and will be
    /// checked on reboot.
    /// - `name`: A name that describes this gateway. Perhaps the EVM network used?
    /// - `transfer_gas_limit`: An optional gas limit used when transferring gas from paid invoices to
    /// the treasury. Useful in case your treasury address is a contract address
    /// that implements custom functionality for handling incoming gas.
    pub fn new<F, Fut>(
        rpc_url: &str,
        treasury_address: String,
        invoice_delay_millis: u64,
        callback: F,
        sled_path: &str,
        name: String,
        transfer_gas_limit: Option<u128>,
    ) -> PaymentGateway
    where
        F: Fn(Invoice) -> Fut + 'static + Send + Sync,
        Fut: Future<Output = ()> + 'static + Send,
    {
        // Send allows ownership to be transferred across threads
        // Sync allows references to be shared

        let db = sled::open(sled_path).unwrap();
        let tree = db.open_tree("invoices").unwrap();
        let provider = ProviderBuilder::new().on_http(Url::from_str(rpc_url).unwrap());

        // Wrap the callback in Arc<Mutex<>> to allow sharing across threads and state mutation
        // We have to create a pinned box to prevent the future from being moved around in heap memory.
        let callback = Arc::new(Mutex::new(move |invoice: Invoice| {
            Box::pin(callback(invoice)) as Pin<Box<dyn Future<Output = ()> + Send>>
        }));

        // Setup logging
        let logfile = FileAppender::builder()
            .encoder(Box::new(PatternEncoder::new("{l} - {m}\n")))
            .build("./acceptevm.log")
            .unwrap();

        let config = Config::builder()
            .appender(Appender::builder().build("logfile", Box::new(logfile)))
            .build(Root::builder().appender("logfile").build(LevelFilter::Info))
            .unwrap();

        // Try to initialize and catch error silently if already initialized
        // during tests this make this function throw error
        if log4rs::init_config(config).is_err() {
            println!("Logger already initialized.");
        }

        // TODO: When implementing token transfers allow the user to add their gas wallet here.

        PaymentGateway {
            config: PaymentGatewayConfiguration {
                provider,
                treasury_address: treasury_address
                    .parse()
                    .unwrap_or_else(|_| panic!("Invalid treasury address")),
                invoice_delay_millis,
                callback,
                transfer_gas_limit,
            },
            tree,
            name,
        }
    }

    /// Retrieves the last invoice
    pub async fn get_last_invoice(&self) -> Result<(String, Invoice), DatabaseError> {
        get_last::<Invoice>(&self.tree).await
    }

    /// Retrieves all invoices in the form of a tuple: String,Invoice
    /// where the first element is the key that was used in the database
    /// and the second part is the invoice. The key is a SHA256 hash of the
    /// creation timestamp and the recipient address.
    pub async fn get_all_invoices(&self) -> Result<Vec<(String, Invoice)>, DatabaseError> {
        get_all::<Invoice>(&self.tree).await
    }

    /// Retrieve an invoice from the payment gateway
    pub async fn get_invoice(&self, key: String) -> Result<Invoice, DatabaseError> {
        get::<Invoice>(&self.tree, &key).await
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
        method: PaymentMethod,
        message: Vec<u8>,
        expires_in_seconds: u64,
    ) -> Result<Invoice, DatabaseError> {
        // Generate random wallet
        let signer = LocalWallet::random();
        let invoice = Invoice {
            to: signer.address().to_string(),
            wallet: types::ZeroizedB256 {
                inner: signer.to_bytes(),
            },
            amount,
            method,
            message,
            paid_at_timestamp: 0,
            expires: get_unix_time_seconds() + expires_in_seconds,
            receipt: None,
        };

        // Create collision-safe key for the map
        let seed = format!("{}{}", signer.address(), get_unix_time_millis());
        let invoice_id = hash_now(seed);
        // Save the invoice in db.
        set::<Invoice>(&self.tree, &invoice_id, invoice.clone()).await?;
        Ok(invoice)
    }
}
