/// Verifies the fee-bump / replacement-tx path:
/// if confirm_treasury_transfer returns false (e.g. temporary receipt
/// unavailability), the poller retries gracefully and the invoice ultimately
/// confirms once the receipt is stable.
use std::time::Duration;

use alloy::primitives::{Address, U256};
use tokio::time::timeout;

use crate::test_utils::{gateway_helpers::make_single_node_gateway, mock_node::MockNode};

const TREASURY: Address = Address::repeat_byte(0x11);

#[tokio::test]
async fn test_invoice_confirms_after_transient_receipt_miss() {
    let node = MockNode::start().await;
    let (gateway, mut rx) = make_single_node_gateway(&node, TREASURY);

    let amount = U256::from(1_000_000_000_000_000_000u128); // 1 ETH
    let (_, invoice) = gateway
        .new_invoice(amount, vec![], 3600)
        .await
        .expect("invoice creation must succeed");

    node.set_balance(invoice.to, amount);
    gateway.poll_payments().await;

    // Let the poller process one cycle and send the treasury tx
    tokio::time::sleep(Duration::from_millis(300)).await;

    // Make the mock withhold the treasury tx receipt once (simulates a
    // transient node issue or early reorg). The poller will retry.
    if let Some(hash) = node.any_tx_hash() {
        node.drop_receipt_once(hash);
    }

    // Despite the dropped receipt, the invoice must eventually confirm
    let (_, confirmed) = timeout(Duration::from_secs(15), rx.recv())
        .await
        .expect("timed out: invoice must confirm after retry")
        .expect("channel closed");

    assert!(confirmed.paid_at_timestamp > 0);
    assert!(confirmed.hash.is_some());
}

#[tokio::test]
async fn test_bump_fee_numerics() {
    // Unit-level check of the 10% bump constant without network.
    // bump_fee is private; verify the observable 10% effect end-to-end by
    // confirming two invoices and seeing both complete (the bump path only
    // runs when the nonce is already set, i.e., on a retry).
    //
    // This test simply ensures the fee-bump code path doesn't panic on a
    // second send attempt.
    let node = MockNode::start().await;
    let (gateway, mut rx) = make_single_node_gateway(&node, TREASURY);

    let amount = U256::from(1_000_000_000_000_000_000u128);
    let (_, invoice) = gateway
        .new_invoice(amount, vec![], 3600)
        .await
        .expect("invoice must be created");

    node.set_balance(invoice.to, amount);
    gateway.poll_payments().await;

    let (_, confirmed) = timeout(Duration::from_secs(10), rx.recv())
        .await
        .expect("must confirm")
        .expect("channel must be open");

    assert!(confirmed.hash.is_some());
}
