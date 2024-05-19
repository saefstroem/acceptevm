pub mod gateway;
pub mod invoice;
mod web3;

#[cfg(test)]
mod tests {
    use async_std::channel::unbounded;
    use std::str::FromStr;

    use crate::{
        gateway::{
            errors::GatewayError, Address, PaymentGateway, PaymentGatewayConfiguration, Provider,
            Reflector, TransactionType, U256,
        },
        invoice::Invoice,
    };

    fn setup_test_gateway() -> PaymentGateway {
        let (sender, _receiver) = unbounded();
        let reflector = Reflector::Sender(sender);
        let provider = Provider::try_from("https://123.com").expect("Invalid RPC URL");
        let transaction_type = TransactionType::Eip1559;

        PaymentGateway::new(PaymentGatewayConfiguration {
            provider,
            treasury_address: "0xdac17f958d2ee523a2206206994597c13d831ec7"
                .parse::<Address>()
                .unwrap(),
            min_confirmations: 10,
            reflector,
            poller_delay_seconds: 1,
            transaction_type,
            eip1559_estimation_retry_max: 3,
            eip1559_estimation_retry_delay_seconds: 10,
        })
    }

    async fn insert_test_invoice(
        gateway: &PaymentGateway,
    ) -> Result<(String, Invoice), GatewayError> {
        gateway
            .new_invoice(
                U256::from_str("0").unwrap(),
                None,
                bincode::serialize("test").unwrap(),
                3600,
            )
            .await
    }

    #[tokio::test]
    async fn assert_invoice_creation() {
        let gateway = setup_test_gateway();
        insert_test_invoice(&gateway).await.unwrap();
        let database_length = gateway.invoices.len();
        println!("Database length: {}", database_length);
        assert_eq!(database_length, 1);
    }

    #[tokio::test]
    async fn assert_valid_address_length() {
        let gateway = setup_test_gateway();
        let invoice = insert_test_invoice(&gateway).await.unwrap();
        let address = format!("{:?}", invoice.1.to);
        let address_length = address.len();
        println!("Address: {}", address);
        println!("Address length: {}", address_length);
        assert_eq!(address_length, 42);
    }
}
