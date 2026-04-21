/// Expired, unfunded invoices must be silently pruned — never sent on the
/// confirmation channel.
use std::time::Duration;

use alloy::primitives::{Address, U256};
use tokio::time::timeout;

use crate::test_utils::{gateway_helpers::make_single_node_gateway, mock_node::MockNode};

const TREASURY: Address = Address::repeat_byte(0xCC);

#[tokio::test]
async fn test_expired_unfunded_invoice_is_pruned() {
    let node = MockNode::start().await;
    let (gateway, mut rx) = make_single_node_gateway(&node, TREASURY);

    // Create invoice with a large TTL, then manually backdate the expiry
    let (id, _) = gateway
        .new_invoice(U256::from(1_000u64), vec![], 3600)
        .await
        .expect("invoice creation must succeed");

    // Backdate expiry to 1 second in the past so it looks expired
    {
        let mut map = gateway.invoices.write().await;
        if let Some(inv) = map.get_mut(&id) {
            inv.expires = 1; // Unix epoch + 1 s — always in the past
        }
    }

    // Do NOT fund the address
    gateway.poll_payments().await;

    // The invoice should NOT appear on the confirmation channel within 3 s
    let result = timeout(Duration::from_secs(3), rx.recv()).await;
    assert!(
        result.is_err(),
        "expired unfunded invoice must not be confirmed"
    );

    // And it must be removed from the map
    tokio::time::sleep(Duration::from_millis(500)).await;
    assert!(
        gateway.get_invoice(&id).await.is_err(),
        "expired invoice must be pruned from the map"
    );
}

#[tokio::test]
async fn test_funded_invoice_before_expiry_confirms() {
    let node = MockNode::start().await;
    let (gateway, mut rx) = make_single_node_gateway(&node, TREASURY);

    let amount = U256::from(1_000_000_000_000_000_000u128); // 1 ETH
    // 1 hour TTL — plenty of time
    let (_, invoice) = gateway
        .new_invoice(amount, vec![], 3600)
        .await
        .expect("invoice creation must succeed");

    node.set_balance(invoice.to, amount);
    gateway.poll_payments().await;

    // Must confirm despite TTL being large
    let result = timeout(Duration::from_secs(10), rx.recv()).await;
    assert!(result.is_ok(), "funded unexpired invoice must confirm");
}
