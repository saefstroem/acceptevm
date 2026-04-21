pub mod gateway;
pub mod invoice;
mod web3;

#[cfg(test)]
mod tests {
    use crate::{
        gateway::{
            error::GatewayError, Address, PaymentGateway, PaymentGatewayConfiguration, U256,
        },
        invoice::Invoice,
    };

    type TestResult = std::result::Result<(), Box<dyn std::error::Error>>;

    fn create_gateway() -> std::result::Result<PaymentGateway, Box<dyn std::error::Error>> {
        let (sender, _receiver) = tokio::sync::mpsc::unbounded_channel();

        Ok(PaymentGateway::new(PaymentGatewayConfiguration {
            rpc_urls: vec!["https://123.com".to_string()],
            treasury_address: "0xdac17f958d2ee523a2206206994597c13d831ec7".parse::<Address>()?,
            min_confirmations: 10,
            sender,
            poller_delay_seconds: 1,
            receipt_timeout_seconds: 60,
        })?)
    }

    async fn insert_test_invoice(
        gateway: &PaymentGateway,
    ) -> Result<(String, Invoice), GatewayError> {
        gateway
            .new_invoice(U256::ZERO, b"test".to_vec(), 3600)
            .await
    }

    #[tokio::test]
    async fn assert_invoice_creation() -> TestResult {
        let gateway = create_gateway()?;
        insert_test_invoice(&gateway).await?;
        let database_length = gateway.invoices.read().await.len();
        assert_eq!(database_length, 1);
        Ok(())
    }

    #[tokio::test]
    async fn assert_multiple_invoices() -> TestResult {
        let gateway = create_gateway()?;
        insert_test_invoice(&gateway).await?;
        insert_test_invoice(&gateway).await?;
        insert_test_invoice(&gateway).await?;
        let count = gateway.invoices.read().await.len();
        assert_eq!(count, 3);
        Ok(())
    }

    #[tokio::test]
    async fn assert_get_invoice_by_id() -> TestResult {
        let gateway = create_gateway()?;
        let (id, original) = insert_test_invoice(&gateway).await?;
        let retrieved = gateway.get_invoice(&id).await?;
        assert_eq!(retrieved.to, original.to);
        assert_eq!(retrieved.amount, original.amount);
        Ok(())
    }

    #[tokio::test]
    async fn assert_get_invoice_not_found() -> TestResult {
        let gateway = create_gateway()?;
        let result = gateway.get_invoice("nonexistent").await;
        assert!(result.is_err());
        Ok(())
    }

    #[tokio::test]
    async fn assert_get_all_invoices() -> TestResult {
        let gateway = create_gateway()?;
        insert_test_invoice(&gateway).await?;
        insert_test_invoice(&gateway).await?;
        let all = gateway.get_all_invoices().await?;
        assert_eq!(all.len(), 2);
        Ok(())
    }

    #[tokio::test]
    async fn assert_unique_invoice_ids() -> TestResult {
        let gateway = create_gateway()?;
        let (id1, _) = insert_test_invoice(&gateway).await?;
        let (id2, _) = insert_test_invoice(&gateway).await?;
        assert_ne!(id1, id2);
        Ok(())
    }

    #[tokio::test]
    async fn assert_unique_wallet_per_invoice() -> TestResult {
        let gateway = create_gateway()?;
        let (_, inv1) = insert_test_invoice(&gateway).await?;
        let (_, inv2) = insert_test_invoice(&gateway).await?;
        assert_ne!(inv1.to, inv2.to);
        assert_ne!(inv1.wallet.inner, inv2.wallet.inner);
        Ok(())
    }

    #[tokio::test]
    async fn assert_invoice_expiry_set() -> TestResult {
        let gateway = create_gateway()?;
        let (_, invoice) = gateway
            .new_invoice(U256::ZERO, b"test".to_vec(), 7200)
            .await?;
        assert!(invoice.expires > 0);
        assert_eq!(invoice.paid_at_timestamp, 0);
        assert!(invoice.hash.is_none());
        assert!(invoice.nonce.is_none());
        Ok(())
    }

    #[tokio::test]
    async fn assert_invoice_message_preserved() -> TestResult {
        let gateway = create_gateway()?;
        let msg = b"hello world".to_vec();
        let (_, invoice) = gateway.new_invoice(U256::ZERO, msg.clone(), 3600).await?;
        assert_eq!(invoice.message, msg);
        Ok(())
    }

    #[tokio::test]
    async fn assert_invoice_amount_preserved() -> TestResult {
        let gateway = create_gateway()?;
        let amount = U256::from(42);
        let (_, invoice) = gateway.new_invoice(amount, b"test".to_vec(), 3600).await?;
        assert_eq!(invoice.amount, amount);
        Ok(())
    }
}
