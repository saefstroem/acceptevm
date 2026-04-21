/// Multiple invoices created and funded simultaneously must all confirm
/// independently.
use std::collections::HashSet;
use std::time::Duration;

use alloy::primitives::{Address, U256};
use tokio::time::timeout;

use crate::test_utils::{gateway_helpers::make_single_node_gateway, mock_node::MockNode};

const TREASURY: Address = Address::repeat_byte(0xFF);

#[tokio::test]
async fn test_five_concurrent_invoices_all_confirm() {
    let node = MockNode::start().await;
    let (gateway, mut rx) = make_single_node_gateway(&node, TREASURY);

    let amount = U256::from(100_000_000_000_000_000u128); // 0.1 ETH each

    // Create and fund 5 invoices
    let mut expected_ids: HashSet<String> = HashSet::new();
    for _ in 0..5 {
        let (id, invoice) = gateway
            .new_invoice(amount, vec![], 3600)
            .await
            .expect("invoice creation must succeed");
        node.set_balance(invoice.to, amount);
        expected_ids.insert(id);
    }

    gateway.poll_payments().await;

    let mut confirmed_ids: HashSet<String> = HashSet::new();
    for _ in 0..5 {
        let result = timeout(Duration::from_secs(15), rx.recv())
            .await
            .expect("timed out waiting for invoice confirmation")
            .expect("channel closed");
        confirmed_ids.insert(result.0);
    }

    assert_eq!(
        confirmed_ids, expected_ids,
        "all 5 invoice IDs must be confirmed"
    );
}

#[tokio::test]
async fn test_mixed_funded_and_unfunded_invoices() {
    let node = MockNode::start().await;
    let (gateway, mut rx) = make_single_node_gateway(&node, TREASURY);

    let amount = U256::from(100_000_000_000_000_000u128); // 0.1 ETH

    // Create 4 invoices; only fund 2 of them
    let mut funded_ids: HashSet<String> = HashSet::new();
    for i in 0..4 {
        let (id, invoice) = gateway
            .new_invoice(amount, vec![], 3600)
            .await
            .expect("invoice creation must succeed");
        if i % 2 == 0 {
            node.set_balance(invoice.to, amount);
            funded_ids.insert(id);
        }
    }

    gateway.poll_payments().await;

    let mut confirmed_ids: HashSet<String> = HashSet::new();
    // Expect exactly 2 confirmations within 10 s
    for _ in 0..2 {
        let result = timeout(Duration::from_secs(10), rx.recv())
            .await
            .expect("timed out: funded invoices must confirm")
            .expect("channel closed");
        confirmed_ids.insert(result.0);
    }

    assert_eq!(confirmed_ids, funded_ids, "only funded invoice IDs must confirm");

    // No more confirmations for the unfunded ones
    let extra = timeout(Duration::from_secs(2), rx.recv()).await;
    assert!(extra.is_err(), "unfunded invoices must not emit confirmations");
}
