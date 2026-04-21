/// Happy-path integration test: invoice is fully funded and the gateway
/// detects the payment, sweeps funds to treasury, and emits a confirmation.
use std::time::Duration;

use alloy::primitives::{Address, U256};
use tokio::time::timeout;

use crate::test_utils::{gateway_helpers::make_single_node_gateway, mock_node::MockNode};

const TREASURY: Address = Address::repeat_byte(0xAB);

#[tokio::test]
async fn test_normal_payment_flow() {
    let node = MockNode::start().await;
    let (gateway, mut rx) = make_single_node_gateway(&node, TREASURY);

    let amount = U256::from(1_000_000_000_000_000_000u128); // 1 ETH
    let (id, invoice) = gateway
        .new_invoice(amount, b"test payment".to_vec(), 3600)
        .await
        .expect("invoice creation must succeed");

    // Fund the invoice address on the mock node
    node.set_balance(invoice.to, amount);

    // Start the poller
    gateway.poll_payments().await;

    // Expect confirmation within 10 seconds
    let (confirmed_id, confirmed_invoice) = timeout(Duration::from_secs(10), rx.recv())
        .await
        .expect("timed out waiting for confirmation")
        .expect("channel closed");

    assert_eq!(confirmed_id, id, "confirmed invoice id must match");
    assert_eq!(confirmed_invoice.to, invoice.to, "deposit address must match");
    assert!(
        confirmed_invoice.paid_at_timestamp > 0,
        "paid_at_timestamp must be set"
    );
    assert!(
        confirmed_invoice.hash.is_some(),
        "treasury tx hash must be set"
    );
}
