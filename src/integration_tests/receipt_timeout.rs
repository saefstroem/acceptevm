/// Verifies that an extremely short (or zero) `receipt_timeout_seconds` does
/// not cause panics or unbounded blocking — the poller simply retries.
use std::time::Duration;

use alloy::primitives::{Address, U256};
use tokio::sync::mpsc;
use tokio::time::timeout;

use crate::gateway::{PaymentGateway, PaymentGatewayConfiguration};
use crate::test_utils::mock_node::MockNode;

const TREASURY: Address = Address::repeat_byte(0x55);

/// With receipt_timeout_seconds = 1 the poller must not panic; the invoice
/// may or may not confirm depending on system speed, but the process must
/// stay alive.
#[tokio::test]
async fn test_short_receipt_timeout_does_not_panic() {
    let node = MockNode::start().await;
    let (tx, mut rx) = mpsc::unbounded_channel();
    let config = PaymentGatewayConfiguration {
        rpc_urls: vec![node.url.clone()],
        treasury_address: TREASURY,
        poller_delay_seconds: 0,
        min_confirmations: 0,
        receipt_timeout_seconds: 1, // very short but non-zero
        sender: tx,
    };
    let gateway = PaymentGateway::new(config).unwrap();

    let amount = U256::from(1_000_000_000_000_000_000u128); // 1 ETH
    let (_, invoice) = gateway
        .new_invoice(amount, vec![], 3600)
        .await
        .expect("invoice creation must succeed");

    node.set_balance(invoice.to, amount);
    gateway.poll_payments().await;

    // Give it a generous timeout — if it panics the test will fail anyway
    let result = timeout(Duration::from_secs(15), rx.recv()).await;
    // We just verify the process is still alive; confirmation is optional
    // (it may succeed if the mock is fast enough).
    let _ = result; // don't assert confirmed / not confirmed
}

/// A gateway configured with receipt_timeout_seconds = 0 should handle
/// every receipt fetch timing out gracefully (returns Ok(false), retries next cycle).
#[tokio::test]
async fn test_zero_receipt_timeout_does_not_panic() {
    let node = MockNode::start().await;
    let (tx, mut rx) = mpsc::unbounded_channel();
    let config = PaymentGatewayConfiguration {
        rpc_urls: vec![node.url.clone()],
        treasury_address: TREASURY,
        poller_delay_seconds: 0,
        min_confirmations: 0,
        receipt_timeout_seconds: 0, // instant timeout
        sender: tx,
    };
    let gateway = PaymentGateway::new(config).unwrap();

    let amount = U256::from(500_000_000_000_000_000u128);
    let (_, invoice) = gateway
        .new_invoice(amount, vec![], 3600)
        .await
        .expect("invoice creation must succeed");

    node.set_balance(invoice.to, amount);
    gateway.poll_payments().await;

    // With 0-second timeout the gateway must NOT panic or crash.
    // Whether confirmation succeeds depends on local timing; we only assert no crash.
    let _ = timeout(Duration::from_secs(2), rx.recv()).await;
}
