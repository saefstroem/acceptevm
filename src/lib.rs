mod web3;
pub mod gateway;
pub mod invoice;

#[cfg(test)]
mod tests {
    use crate::{
        gateway::{
            error::GatewayError, Address, PaymentGateway, PaymentGatewayConfiguration, U256,
        },
        invoice::Invoice,
    };

    fn setup_test_gateway() -> PaymentGateway {
        let (sender, _receiver) = tokio::sync::mpsc::unbounded_channel();

        PaymentGateway::new(PaymentGatewayConfiguration {
            native_currency_name: "ETH".to_string(),
            rpc_url: "https://123.com".to_string(),
            treasury_address: "0xdac17f958d2ee523a2206206994597c13d831ec7"
                .parse::<Address>()
                .unwrap(),
            min_confirmations: 10,
            sender,
            poller_delay_seconds: 1,
        })
    }

    async fn insert_test_invoice(
        gateway: &PaymentGateway,
    ) -> Result<(String, Invoice), GatewayError> {
        gateway
            .new_invoice(U256::ZERO, b"test".to_vec(), 3600)
            .await
    }

    #[tokio::test]
    async fn assert_invoice_creation() {
        let gateway = setup_test_gateway();
        insert_test_invoice(&gateway).await.unwrap();
        let database_length = gateway.invoices.read().await.len();
        println!("Database length: {}", database_length);
        assert_eq!(database_length, 1);
    }

    #[tokio::test]
    async fn assert_multiple_invoices() {
        let gateway = setup_test_gateway();
        insert_test_invoice(&gateway).await.unwrap();
        insert_test_invoice(&gateway).await.unwrap();
        insert_test_invoice(&gateway).await.unwrap();
        let count = gateway.invoices.read().await.len();
        assert_eq!(count, 3);
    }

    #[tokio::test]
    async fn assert_get_invoice_by_id() {
        let gateway = setup_test_gateway();
        let (id, original) = insert_test_invoice(&gateway).await.unwrap();
        let retrieved = gateway.get_invoice(&id).await.unwrap();
        assert_eq!(retrieved.to, original.to);
        assert_eq!(retrieved.amount, original.amount);
    }

    #[tokio::test]
    async fn assert_get_invoice_not_found() {
        let gateway = setup_test_gateway();
        let result = gateway.get_invoice("nonexistent").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn assert_get_all_invoices() {
        let gateway = setup_test_gateway();
        insert_test_invoice(&gateway).await.unwrap();
        insert_test_invoice(&gateway).await.unwrap();
        let all = gateway.get_all_invoices().await.unwrap();
        assert_eq!(all.len(), 2);
    }

    #[tokio::test]
    async fn assert_unique_invoice_ids() {
        let gateway = setup_test_gateway();
        let (id1, _) = insert_test_invoice(&gateway).await.unwrap();
        let (id2, _) = insert_test_invoice(&gateway).await.unwrap();
        assert_ne!(id1, id2);
    }

    #[tokio::test]
    async fn assert_unique_wallet_per_invoice() {
        let gateway = setup_test_gateway();
        let (_, inv1) = insert_test_invoice(&gateway).await.unwrap();
        let (_, inv2) = insert_test_invoice(&gateway).await.unwrap();
        assert_ne!(inv1.to, inv2.to);
        assert_ne!(inv1.wallet.inner, inv2.wallet.inner);
    }

    #[tokio::test]
    async fn assert_invoice_expiry_set() {
        let gateway = setup_test_gateway();
        let (_, invoice) = gateway
            .new_invoice(U256::ZERO, b"test".to_vec(), 7200)
            .await
            .unwrap();
        assert!(invoice.expires > 0);
        assert_eq!(invoice.paid_at_timestamp, 0);
        assert!(invoice.hash.is_none());
    }

    #[tokio::test]
    async fn assert_invoice_message_preserved() {
        let gateway = setup_test_gateway();
        let msg = b"hello world".to_vec();
        let (_, invoice) = gateway
            .new_invoice(U256::ZERO, msg.clone(), 3600)
            .await
            .unwrap();
        assert_eq!(invoice.message, msg);
    }

    #[tokio::test]
    async fn assert_invoice_amount_preserved() {
        let gateway = setup_test_gateway();
        let amount = U256::from(42);
        let (_, invoice) = gateway
            .new_invoice(amount, b"test".to_vec(), 3600)
            .await
            .unwrap();
        assert_eq!(invoice.amount, amount);
    }
}
