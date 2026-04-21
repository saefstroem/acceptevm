/// Verifies that confirmed payments are correctly swept to the treasury address
/// and that the treasury balance increases by approximately the invoice amount.
use std::time::Duration;

use alloy::primitives::{Address, U256};
use tokio::time::timeout;

use crate::test_utils::{gateway_helpers::make_single_node_gateway, mock_node::MockNode};

const TREASURY: Address = Address::repeat_byte(0x44);

#[tokio::test]
async fn test_treasury_receives_funds_after_confirmation() {
    let node = MockNode::start().await;

    // Ensure treasury starts with zero balance
    assert_eq!(node.get_balance(TREASURY), U256::ZERO);

    let (gateway, mut rx) = make_single_node_gateway(&node, TREASURY);

    let amount = U256::from(2_000_000_000_000_000_000u128); // 2 ETH
    let (_, invoice) = gateway
        .new_invoice(amount, vec![], 3600)
        .await
        .expect("invoice creation must succeed");

    node.set_balance(invoice.to, amount);
    gateway.poll_payments().await;

    let _ = timeout(Duration::from_secs(10), rx.recv())
        .await
        .expect("invoice must confirm")
        .expect("channel closed");

    let treasury_bal = node.get_balance(TREASURY);
    assert!(
        treasury_bal > U256::ZERO,
        "treasury must receive a non-zero balance after confirmation"
    );
    // The treasury sweep sends balance - gas; gas is ~21000 * 1 gwei = 21000 gwei
    // So treasury should hold at least amount - 1_000_000 (1 milli-eth slack)
    let min_expected = amount - U256::from(1_000_000_000_000_000u128);
    assert!(
        treasury_bal >= min_expected,
        "treasury balance {treasury_bal} should be close to invoice amount {amount}"
    );
}

#[tokio::test]
async fn test_invoice_address_drained_after_sweep() {
    let node = MockNode::start().await;
    let (gateway, mut rx) = make_single_node_gateway(&node, TREASURY);

    let amount = U256::from(1_000_000_000_000_000_000u128); // 1 ETH
    let (_, invoice) = gateway
        .new_invoice(amount, vec![], 3600)
        .await
        .expect("invoice creation must succeed");

    node.set_balance(invoice.to, amount);
    gateway.poll_payments().await;

    let _ = timeout(Duration::from_secs(10), rx.recv())
        .await
        .expect("invoice must confirm")
        .expect("channel closed");

    // The one-time deposit address must be fully drained after the sweep
    let deposit_bal = node.get_balance(invoice.to);
    assert_eq!(
        deposit_bal,
        U256::ZERO,
        "deposit address must be empty after treasury sweep"
    );
}
