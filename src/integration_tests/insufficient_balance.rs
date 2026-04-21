/// Insufficient balance: an underfunded invoice must not confirm; once topped up
/// it should confirm normally.
use std::time::Duration;

use alloy::primitives::{Address, U256};
use tokio::time::timeout;

use crate::test_utils::{gateway_helpers::make_single_node_gateway, mock_node::MockNode};

const TREASURY: Address = Address::repeat_byte(0xDD);

#[tokio::test]
async fn test_underfunded_invoice_does_not_confirm() {
    let node = MockNode::start().await;
    let (gateway, mut rx) = make_single_node_gateway(&node, TREASURY);

    let amount = U256::from(1_000_000_000_000_000_000u128); // 1 ETH
    let (_, invoice) = gateway
        .new_invoice(amount, vec![], 3600)
        .await
        .expect("invoice creation must succeed");

    // Fund with one wei less than required
    node.set_balance(invoice.to, amount - U256::from(1u64));

    gateway.poll_payments().await;

    // Must NOT confirm within 3 seconds
    let result = timeout(Duration::from_secs(3), rx.recv()).await;
    assert!(
        result.is_err(),
        "underfunded invoice must not be confirmed"
    );
}

#[tokio::test]
async fn test_topped_up_invoice_eventually_confirms() {
    let node = MockNode::start().await;
    let (gateway, mut rx) = make_single_node_gateway(&node, TREASURY);

    let amount = U256::from(1_000_000_000_000_000_000u128); // 1 ETH
    let (_, invoice) = gateway
        .new_invoice(amount, vec![], 3600)
        .await
        .expect("invoice creation must succeed");

    // Initially underfunded
    node.set_balance(invoice.to, amount - U256::from(1u64));

    gateway.poll_payments().await;

    // Let the poller see the under-funded amount
    tokio::time::sleep(Duration::from_millis(300)).await;

    // Top up to the full amount
    node.set_balance(invoice.to, amount);

    // Now it should confirm
    let result = timeout(Duration::from_secs(10), rx.recv()).await;
    assert!(
        result.is_ok(),
        "after top-up the invoice must eventually confirm"
    );
    let (_, confirmed) = result.unwrap().unwrap();
    assert!(confirmed.paid_at_timestamp > 0);
}
