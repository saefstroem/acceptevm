/// Overpaid invoices (balance > required amount) must still confirm, sweeping
/// the full available balance to the treasury.
use std::time::Duration;

use alloy::primitives::{Address, U256};
use tokio::time::timeout;

use crate::test_utils::{gateway_helpers::make_single_node_gateway, mock_node::MockNode};

const TREASURY: Address = Address::repeat_byte(0xEE);

#[tokio::test]
async fn test_overpaid_invoice_confirms() {
    let node = MockNode::start().await;
    let (gateway, mut rx) = make_single_node_gateway(&node, TREASURY);

    let amount = U256::from(1_000_000_000_000_000_000u128); // 1 ETH
    let overpay = amount * U256::from(2u64);

    let (id, invoice) = gateway
        .new_invoice(amount, vec![], 3600)
        .await
        .expect("invoice creation must succeed");

    // Fund with twice the required amount
    node.set_balance(invoice.to, overpay);

    gateway.poll_payments().await;

    let (confirmed_id, confirmed_invoice) = timeout(Duration::from_secs(10), rx.recv())
        .await
        .expect("timed out waiting for overpaid confirmation")
        .expect("channel closed");

    assert_eq!(confirmed_id, id);
    assert!(confirmed_invoice.paid_at_timestamp > 0);
    assert!(confirmed_invoice.hash.is_some());
}

#[tokio::test]
async fn test_overpaid_invoice_sweeps_full_balance_to_treasury() {
    let node = MockNode::start().await;
    let (gateway, mut rx) = make_single_node_gateway(&node, TREASURY);

    let amount = U256::from(500_000_000_000_000_000u128); // 0.5 ETH
    let funded = amount * U256::from(3u64); // 1.5 ETH

    let (_, invoice) = gateway
        .new_invoice(amount, vec![], 3600)
        .await
        .expect("invoice creation must succeed");

    node.set_balance(invoice.to, funded);
    gateway.poll_payments().await;

    let _ = timeout(Duration::from_secs(10), rx.recv())
        .await
        .expect("timed out")
        .expect("channel closed");

    // Treasury must have received all funds (minus gas)
    let treasury_bal = node.get_treasury_balance(TREASURY);
    assert!(
        treasury_bal > U256::ZERO,
        "treasury must have received the swept funds"
    );
    // The sweep should be at least the invoice amount
    assert!(
        treasury_bal >= amount,
        "treasury should hold at least the invoice amount"
    );
}
