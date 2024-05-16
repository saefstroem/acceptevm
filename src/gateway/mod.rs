mod hash;
use std::{future::Future, pin::Pin, str::FromStr, sync::Arc};

use alloy::{
    primitives::{Address, U256},
    providers::{ProviderBuilder, RootProvider},
    signers::wallet::LocalWallet,
    transports::http::Http,
};

use async_std::channel::Sender;
use reqwest::{Client, Url};
use sled::Tree;

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
    pub reflector: Reflector,
    pub transfer_gas_limit: Option<u128>,
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
    Sender(Sender<Invoice>),
}

// Type alias for the underlying Web3 type.
pub type Wei = U256;

// Type alias for the invoice callback function
pub type AsyncCallback =
    Arc<dyn Fn(Invoice) -> Pin<Box<dyn Future<Output = ()> + Send>> + Send + Sync>;

impl PaymentGateway {
    /// Creates a new payment gateway.
    ///
    /// - `rpc_url`: the HTTP Rpc url of the EVM network
    /// - `treasury_address`: the address of the treasury for all paid invoices, on this EVM network.
    /// - `invoice_delay_millis`: how long to wait before checking the next invoice in milliseconds.
    /// This is used to prevent potential rate limits from the node.
    /// - `reflector`: The reflector is an enum that allows you to receive the paid invoices.
    /// At the moment, the only reflector available is the `Sender` from the async-std channel.
    /// This means that you will need to create a channel and pass the sender as the reflector.
    /// - `sled_path`: The path of the sled database where the pending invoices will
    /// be stored. In the event of a crash the invoices are saved and will be
    /// checked on reboot.
    /// - `name`: A name that describes this gateway. Perhaps the EVM network used?
    /// - `transfer_gas_limit`: An optional gas limit used when transferring gas from paid invoices to
    /// the treasury. Useful in case your treasury address is a contract address
    /// that implements custom functionality for handling incoming gas.
    ///
    /// Example:
    /// ```rust
    /// use acceptevm::gateway::{PaymentGateway, Reflector};
    /// use async_std::channel::unbounded;
    /// let (sender, _receiver) = unbounded();
    /// let reflector = Reflector::Sender(sender);
    ///
    /// PaymentGateway::new(
    ///        "https://123.com",
    ///        "0xdac17f958d2ee523a2206206994597c13d831ec7".to_string(),
    ///        10,
    ///        reflector,
    ///        "./your-wanted-db-path",
    ///        "test".to_string(),
    ///        Some(21000),
    /// );
    /// ```


    pub fn new(
        rpc_url: &str,
        treasury_address: String,
        invoice_delay_millis: u64,
        reflector: Reflector,
        sled_path: &str,
        name: String,
        transfer_gas_limit: Option<u128>,
    ) -> PaymentGateway {
        let db = sled::open(sled_path).unwrap();
        let tree = db.open_tree("invoices").unwrap();
        let provider = ProviderBuilder::new().on_http(Url::from_str(rpc_url).unwrap());

        // TODO: When implementing token transfers allow the user to add their gas wallet here.

        PaymentGateway {
            config: PaymentGatewayConfiguration {
                provider,
                treasury_address: treasury_address
                    .parse()
                    .unwrap_or_else(|_| panic!("Invalid treasury address")),
                invoice_delay_millis,
                reflector,
                transfer_gas_limit,
            },
            tree,
            name,
        }
    }

    /// Retrieves the last invoice
    pub async fn get_last_invoice(&self) -> Result<(String, Invoice), DatabaseError> {
        get_last(&self.tree).await
    }

    /// Retrieves all invoices in the form of a tuple: String,Invoice
    /// where the first element is the key that was used in the database
    /// and the second part is the invoice. The key is a SHA256 hash of the
    /// creation timestamp and the recipient address.
    pub async fn get_all_invoices(&self) -> Result<Vec<(String, Invoice)>, DatabaseError> {
        get_all(&self.tree).await
    }

    /// Retrieve an invoice from the payment gateway
    pub async fn get_invoice(&self, key: String) -> Result<Invoice, DatabaseError> {
        get(&self.tree, &key).await
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
        set(&self.tree, &invoice_id, &invoice).await?;
        Ok(invoice)
    }
}
