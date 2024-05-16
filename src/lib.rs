mod common;
mod db;
mod erc20;
pub mod gateway;
mod poller;
mod transfers;
pub mod types;

#[cfg(test)]
mod tests {
    use std::{fs, path::Path, str::FromStr};

    use alloy::primitives::U256;
    use async_std::channel::unbounded;

    use crate::{
        common::DatabaseError,
        gateway::{PaymentGateway, Reflector},
        types::{Invoice, PaymentMethod},
    };

    fn setup_test_gateway(db_path: &str) -> PaymentGateway {
        let (sender, _receiver) = unbounded();
        let reflector = Reflector::Sender(sender);

        PaymentGateway::new(
            "https://123.com",
            "0xdac17f958d2ee523a2206206994597c13d831ec7".to_string(),
            10,
            reflector,
            db_path,
            "test".to_string(),
            Some(21000),
        )
    }

    fn remove_test_db(db_path: &str) {
        if Path::new(db_path).exists() {
            fs::remove_dir_all(db_path).expect("Failed to remove test database");
        }
    }

    async fn insert_test_invoice(gateway: &PaymentGateway) -> Result<Invoice, DatabaseError> {
        gateway
            .new_invoice(
                U256::from_str("0").unwrap(),
                PaymentMethod {
                    token_address: None,
                },
                bincode::serialize("test").unwrap(),
                3600,
            )
            .await
    }

    #[tokio::test]
    async fn assert_invoice_creation() {
        let gateway = setup_test_gateway("./test-assert-invoice-creation");
        insert_test_invoice(&gateway).await.unwrap();
        let database_length = gateway.tree.len();
        println!("Database length: {}", database_length);
        assert_eq!(database_length, 1);
        remove_test_db("./test-assert-invoice-creation");
    }

    #[tokio::test]
    async fn assert_valid_address_length() {
        let gateway = setup_test_gateway("./test-assert-valid-address-length");
        let invoice = insert_test_invoice(&gateway).await.unwrap();
        let address_length = invoice.to.len();
        println!("Address length: {}", address_length);
        assert_eq!(address_length, 42);
        remove_test_db("./test-assert-valid-address-length");
    }
    
}
