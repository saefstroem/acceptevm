#![warn(missing_docs)]

//! This is the main entry point for the payment
//! gateway library

use sled::{Db, Tree};
mod common;

/// Describes the structure of a payment method in
/// a gateway
pub struct PaymentMethod {
    /// Whether or not the method uses native gas token
    pub is_native: bool,
    /// The address of the ERC20 token
    pub token_address: Option<String>,
    /// N decimals used in the ERC20 token, to save bandwidth
    /// we hardcode this value instead of dynamically fetching it
    pub decimals: u64,
}

pub struct Invoice {
    pub amount: u64,
    pub method: PaymentMethod,
    pub message: Vec<u8>,
    pub paid_at_timestamp: u64,
}

pub struct PaymentGateway {
    pub payment_method: PaymentMethod,
    pub rpc_url: String,
    pub poll_interval_seconds: u64,
    pub callback: fn(Invoice),
    pub tree: Tree,
}

impl PaymentGateway {
    pub fn new(
        payment_method: PaymentMethod,
        rpc_url: String,
        poll_interval_seconds: u64,
        callback: fn(Invoice),
        sled_path:String
    ) -> PaymentGateway{
        let db = sled::open(sled_path).unwrap();
        let tree = db.open_tree("invoices").unwrap();
        PaymentGateway {
            payment_method:payment_method,
            rpc_url:rpc_url,
            poll_interval_seconds:poll_interval_seconds,
            callback:callback,
            tree:tree
        }
    }

    pub fn get_last_invoice(&self) -> u32 {
        for el in self.tree.iter() {
            match el {
                Ok(value)=>{
                    let el_bin_key=value.0.to_vec();
                    let x:u32= el_bin_key.try_into().unwrap();
                },
                Err(error)=>{
                    return 0;
                }
            }
        }
        return 0;
    }

    pub fn poll_payments(&self) {}

    pub fn new_invoice(message: Vec<u8>) {}
}

#[cfg(test)]
mod tests {
    use crate::{Invoice, PaymentGateway, PaymentMethod};

    #[test]
    fn sled_creates_db() {
        assert_eq!(0, 0);
    }
}
