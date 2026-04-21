/// Verifies that when multiple RPC URLs are configured, the gateway
/// distributes requests across them (round-robin), and that the URL
/// selection wraps correctly.
use std::time::Duration;

use alloy::primitives::{Address, U256};
use tokio::time::timeout;

use crate::gateway::{PaymentGateway, PaymentGatewayConfiguration};
use crate::test_utils::mock_node::MockNode;

const TREASURY: Address = Address::repeat_byte(0x33);

/// Verifies that `next_rpc_url` cycles through all configured URLs and wraps
/// around, never going out of bounds.
#[tokio::test]
async fn test_round_robin_index_wraps() {
    use tokio::sync::mpsc;

    let node = MockNode::start().await;
    let (tx, _rx) = mpsc::unbounded_channel();
    let urls = vec![
        node.url.clone(),
        "http://fake-a.invalid".to_string(),
        "http://fake-b.invalid".to_string(),
    ];
    let config = PaymentGatewayConfiguration {
        rpc_urls: urls.clone(),
        treasury_address: TREASURY,
        poller_delay_seconds: 0,
        min_confirmations: 0,
        receipt_timeout_seconds: 1,
        sender: tx,
    };
    let gateway = PaymentGateway::new(config).unwrap();

    // Call 6 times — should visit all 3 URLs twice in order
    let results: Vec<String> = (0..6)
        .map(|_| gateway.next_rpc_url().to_string())
        .collect();

    assert_eq!(results[0], urls[0]);
    assert_eq!(results[1], urls[1]);
    assert_eq!(results[2], urls[2]);
    assert_eq!(results[3], urls[0], "must wrap back to first URL");
    assert_eq!(results[4], urls[1]);
    assert_eq!(results[5], urls[2]);
}

/// Ensure many concurrent invoices can all confirm even when round-robin
/// sends different poll calls to the same back-end node.
#[tokio::test]
async fn test_round_robin_single_real_node_three_urls() {
    use tokio::sync::mpsc;

    // Register the same node under three "different" URLs by using
    // localhost with the same port — the round-robin will cycle across them
    // but all calls land on the same mock.
    let node = MockNode::start().await;
    let (tx, mut rx) = mpsc::unbounded_channel();
    let config = PaymentGatewayConfiguration {
        rpc_urls: vec![node.url.clone(), node.url.clone(), node.url.clone()],
        treasury_address: TREASURY,
        poller_delay_seconds: 0,
        min_confirmations: 0,
        receipt_timeout_seconds: 5,
        sender: tx,
    };
    let gateway = PaymentGateway::new(config).unwrap();

    let amount = U256::from(200_000_000_000_000_000u128); // 0.2 ETH
    for _ in 0..3 {
        let (_, inv) = gateway
            .new_invoice(amount, vec![], 3600)
            .await
            .expect("invoice creation must succeed");
        node.set_balance(inv.to, amount);
    }

    gateway.poll_payments().await;

    for _ in 0..3 {
        let _ = timeout(Duration::from_secs(15), rx.recv())
            .await
            .expect("all invoices must confirm");
    }

    // The node must have received far more than 3 requests (one per poll step)
    assert!(
        node.request_count() > 3,
        "round-robin should have produced many RPC calls"
    );
}
