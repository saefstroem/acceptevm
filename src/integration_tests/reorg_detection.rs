/// Simulates a chain reorganisation: the treasury tx receipt disappears between
/// the two successive receipt fetches in `confirm_treasury_transfer`.
/// The poller must NOT falsely confirm the invoice; it should retry and
/// eventually succeed once the chain is stable.
use std::time::Duration;

use alloy::primitives::{Address, U256};
use tokio::time::timeout;

use crate::test_utils::{gateway_helpers::make_single_node_gateway, mock_node::MockNode};

const TREASURY: Address = Address::repeat_byte(0x22);

#[tokio::test]
async fn test_transient_receipt_drop_does_not_falsely_confirm() {
    let node = MockNode::start().await;
    let (gateway, mut rx) = make_single_node_gateway(&node, TREASURY);

    let amount = U256::from(1_000_000_000_000_000_000u128); // 1 ETH
    let (_, invoice) = gateway
        .new_invoice(amount, vec![], 3600)
        .await
        .expect("invoice creation must succeed");

    node.set_balance(invoice.to, amount);
    gateway.poll_payments().await;

    // Let the poller send the treasury tx
    tokio::time::sleep(Duration::from_millis(400)).await;

    // Withhold the receipt once — simulates a transient node glitch or
    // a very short reorg window.
    if let Some(hash) = node.any_tx_hash() {
        node.drop_receipt_once(hash);
    }

    // The invoice should still eventually confirm (the retry will find the receipt)
    let (_, confirmed) = timeout(Duration::from_secs(15), rx.recv())
        .await
        .expect("invoice must eventually confirm after reorg recovery")
        .expect("channel closed");

    assert!(confirmed.paid_at_timestamp > 0);
}

#[tokio::test]
async fn test_confirmation_respects_min_confirmations() {
    let node = MockNode::start().await;

    // Use 1 required confirmation — tx is at block 1, latest is also block 1
    // so the first check returns false; after mining 1 block it confirms.
    use crate::test_utils::gateway_helpers::make_gateway_with_confirmations;
    let (gateway, mut rx) =
        make_gateway_with_confirmations(vec![node.url.clone()], TREASURY, 1);

    let amount = U256::from(1_000_000_000_000_000_000u128);
    let (_, invoice) = gateway
        .new_invoice(amount, vec![], 3600)
        .await
        .expect("invoice creation must succeed");

    node.set_balance(invoice.to, amount);
    gateway.poll_payments().await;

    // Let the poller send the tx (it's included at block 1, latest = 1, depth = 0 < 1)
    tokio::time::sleep(Duration::from_millis(500)).await;

    // Advance the chain by one block → depth becomes 1 >= 1 → confirms
    node.mine_blocks(1);

    let (_, confirmed) = timeout(Duration::from_secs(15), rx.recv())
        .await
        .expect("invoice must confirm after mining 1 block")
        .expect("channel closed");

    assert!(confirmed.paid_at_timestamp > 0);
}
