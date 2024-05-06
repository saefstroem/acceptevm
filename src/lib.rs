mod audit;
mod poller;
pub mod gateway;
pub mod types;
mod erc20;

#[cfg(test)]
mod tests {
    use std::{env, fs, path::Path};

    use sled::Error;
    use web3::types::U256;

    use crate::{gateway::PaymentGateway, types::{Invoice, PaymentMethod}};

    fn setup_test_gateway(db_path: &str) -> PaymentGateway {
        async fn callback(_invoice: Invoice) {}
        PaymentGateway::new("https://123.com", 10, callback, db_path)
    }


    fn remove_test_db(db_path: &str) {
        if Path::new(db_path).exists() {
            fs::remove_dir_all(db_path).expect("Failed to remove test database");
        }
    }

    async fn insert_test_invoice(gateway: &PaymentGateway) -> Result<Invoice, Error> {
        gateway
            .new_invoice(
                U256::one(),
                PaymentMethod {
                    is_native: true,
                    token_address: None,
                },
                bincode::serialize("test").unwrap(),
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
