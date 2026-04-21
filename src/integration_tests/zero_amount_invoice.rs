/// Zero-amount invoices must be confirmed instantly without any on-chain check.
use std::time::Duration;

use alloy::primitives::{Address, U256};
use tokio::time::timeout;

use crate::test_utils::{gateway_helpers::make_single_node_gateway, mock_node::MockNode};

const TREASURY: Address = Address::repeat_byte(0xBA);

#[tokio::test]
async fn test_zero_amount_invoice_instant_confirm() {
    let node = MockNode::start().await;
    let (gateway, mut rx) = make_single_node_gateway(&node, TREASURY);

    // Create a zero-amount invoice — no funding needed
    let (id, invoice) = gateway
        .new_invoice(U256::ZERO, b"free".to_vec(), 3600)
        .await
        .expect("invoice creation must succeed");

    gateway.poll_payments().await;

    let (confirmed_id, confirmed_invoice) = timeout(Duration::from_secs(5), rx.recv())
        .await
        .expect("timed out waiting for zero-amount confirmation")
        .expect("channel closed");

    assert_eq!(confirmed_id, id);
    assert_eq!(confirmed_invoice.to, invoice.to);
    assert!(confirmed_invoice.paid_at_timestamp > 0);
    // No treasury tx for zero-amount invoices
    assert!(confirmed_invoice.hash.is_none());
}

#[tokio::test]
async fn test_zero_amount_invoice_not_stored_after_confirm() {
    let node = MockNode::start().await;
    let (gateway, mut rx) = make_single_node_gateway(&node, TREASURY);

    let (id, _) = gateway
        .new_invoice(U256::ZERO, vec![], 3600)
        .await
        .expect("invoice creation must succeed");

    assert_eq!(gateway.invoices.read().await.len(), 1);

    gateway.poll_payments().await;
    let _ = timeout(Duration::from_secs(5), rx.recv()).await;

    // Invoice must have been removed from the map after confirmation
    assert!(
        gateway.get_invoice(&id).await.is_err(),
        "confirmed invoice must be removed from the map"
    );
}
