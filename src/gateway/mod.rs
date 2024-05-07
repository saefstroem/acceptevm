mod hash;
use std::{future::Future, pin::Pin, sync::Arc,};

use alloy::signers::wallet::LocalWallet;
use sled::Tree;
use tokio::sync::Mutex;
use web3::{transports::Http, types::U256, Web3};

use crate::{
    common::{get_unix_time_millis, get_unix_time_seconds, GetError, SetError}, db::{ get, get_all, get_last, set}, poller::poll_payments, types::{Invoice, PaymentMethod}
};

use self::hash::hash_now;

/// ## AcceptEVM
/// 
/// 
/// The payment gateway is designed to be ran on the main thread, majority of 
/// the functions are non-blocking asynchronous functions. The underlying polling
/// mechanism is offloaded using `tokio::spawn``
#[derive(Clone)]
pub struct PaymentGateway {
    pub web3: Web3<Http>,
    pub invoice_delay_millis: u64,
    pub callback: AsyncCallback,
    pub tree: Tree,
    pub name:String
}

pub type Wei=U256; 
pub type AsyncCallback = Arc<Mutex<dyn Fn(Invoice) -> Pin<Box<dyn Future<Output = ()> + Send>> + Send + Sync>>;

impl PaymentGateway {

    /// Creates a new payment gateway. 
    /// 
    /// - **rpc_url**: the HTTP Rpc url of the EVM network
    /// - **invoice_delay_millis**: how long to wait before checking the next invoice in milliseconds.
    /// This is used to prevent potential rate limits from the node.
    /// - **callback**: an async function that is called when an invoice is paid.
    /// - **sled_path**: the path of the sled database where the pending invoices will
    /// be stored. In the event of a crash the invoices are saved and will be
    /// checked on reboot.
    pub fn new<F, Fut>(
        rpc_url: &str,
        invoice_delay_millis: u64,
        callback: F,
        sled_path: &str,
        name:String
    ) -> PaymentGateway
    where
        F: Fn(Invoice) -> Fut + 'static + Send + Sync,
        Fut: Future<Output = ()> + 'static + Send,
    {
        let db = sled::open(sled_path).unwrap();
        let tree = db.open_tree("invoices").unwrap();
        let http = Http::new(rpc_url).unwrap();
        
        // Wrap the callback in Arc<Mutex<>> to allow sharing across threads and state mutation
        let callback = Arc::new(Mutex::new(move |invoice: Invoice| {
            Box::pin(callback(invoice)) as Pin<Box<dyn Future<Output = ()> + Send>>
        }));
        
        PaymentGateway {
            web3: Web3::new(http),
            invoice_delay_millis,
            callback,
            tree,
            name
        }
    }


    /// Retrieves the last invoice 
    pub async fn get_last_invoice(&self) -> Result<(String, Invoice), GetError> {
        Ok(get_last::<Invoice>(&self.tree).await?)
    }

    /// Retrieves all invoices in the form of a tuple: String,Invoice
    /// where the first element is the key that was used in the database
    /// and the second part is the invoice. The key is a SHA256 hash of the 
    /// creation timestamp and the recipient address.
    pub async fn get_all_invoices(&self) -> Result<Vec<(String, Invoice)>, GetError> {
        Ok(get_all::<Invoice>(&self.tree).await?)
    }

    /// Retrieve an invoice from the payment gateway
    pub async fn get_invoice(&self, key: String) -> Result<Invoice, GetError> {
        Ok(get::<Invoice>(&self.tree, &key).await?)
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
        expires_in_seconds:u64
    ) -> Result<Invoice, SetError> {
        let signer = LocalWallet::random();
        let address = signer.address();
        let invoice = Invoice {
            to: address.to_string(),
            amount: amount,
            method: method,
            message: message,
            paid_at_timestamp: 0,
            expires:get_unix_time_seconds()+expires_in_seconds

        };
        let seed = format!(
            "{}{}",
            signer.address().to_string(),
            get_unix_time_millis().to_string()
        );
        let invoice_id = hash_now(seed);

        set::<Invoice>(&self.tree, &invoice_id, invoice.clone()).await?;
        Ok(invoice)
    }
}
