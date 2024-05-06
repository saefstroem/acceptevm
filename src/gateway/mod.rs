pub mod db;
mod hash;
use std::{future::Future, pin::Pin, sync::Arc,};

use alloy::signers::wallet::LocalWallet;
use sled::{Error, Tree};
use tokio::sync::Mutex;
use web3::{transports::Http, types::U256, Web3};

use crate::{
    audit::logger::get_unix_time_millis,
    poller::poll_payments,
    types::{Invoice, PaymentMethod, Serializable},
};

use self::{
    db::{errors::GetError, get, get_all, get_last},
    hash::hash_now,
};

/// ## AcceptEVM
/// 
/// 
/// The payment gateway is designed to be ran on the main thread, majority of 
/// the functions are non-blocking asynchronous functions. The underlying polling
/// mechanism is offloaded using `tokio::spawn``
#[derive(Clone)]
pub struct PaymentGateway {
    pub web3: Web3<Http>,
    pub poll_interval_seconds: u64,
    pub callback: AsyncCallback,
    pub tree: Tree,
}

pub type AsyncCallback = Arc<Mutex<dyn Fn(Invoice) -> Pin<Box<dyn Future<Output = ()> + Send>> + Send + Sync>>;

impl PaymentGateway {
    pub fn new<F, Fut>(
        rpc_url: &str,
        poll_interval_seconds: u64,
        callback: F,
        sled_path: &str,
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
            poll_interval_seconds,
            callback,
            tree,
        }
    }


    /// Retrieves the last invoice 
    pub async fn get_last_invoice(&self) -> Result<(String, Invoice), GetError> {
        Ok(get_last::<Invoice>(&self.tree).await?)
    }

    pub async fn get_all_invoices(&self) -> Result<Vec<(String, Invoice)>, GetError> {
        Ok(get_all::<Invoice>(&self.tree).await?)
    }

    pub async fn get_invoice(&self, key: String) -> Result<Invoice, GetError> {
        Ok(get::<Invoice>(&self.tree, &key).await?)
    }

    pub async fn poll_payments(&self) {
        let gateway = self.clone();
        tokio::spawn(poll_payments(gateway));
    }

    pub async fn new_invoice(
        &self,
        amount: U256,
        method: PaymentMethod,
        message: Vec<u8>,
    ) -> Result<Invoice, Error> {
        let signer = LocalWallet::random();
        let address = signer.address();
        let invoice = Invoice {
            to: address.to_string(),
            amount: amount,
            method: method,
            message: message,
            paid_at_timestamp: 0,
        };
        let seed = format!(
            "{}{}",
            signer.address().to_string(),
            get_unix_time_millis().to_string()
        );
        let invoice_id = hash_now(seed);
        self.tree.insert(invoice_id, invoice.to_bin().unwrap())?;
        Ok(invoice)
    }
}
