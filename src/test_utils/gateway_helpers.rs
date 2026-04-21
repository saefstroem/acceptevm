use alloy::primitives::Address;
use tokio::sync::mpsc::{self, UnboundedReceiver};

use crate::gateway::{PaymentGateway, PaymentGatewayConfiguration};
use crate::invoice::Invoice;

use super::mock_node::MockNode;

/// Build a `PaymentGateway` wired to the given mock node URL(s).
///
/// Uses `min_confirmations = 0` so tests don't need to manually mine blocks;
/// pass a custom config for tests that specifically exercise confirmation depth.
pub fn make_gateway(
    rpc_urls: Vec<String>,
    treasury_address: Address,
) -> (PaymentGateway, UnboundedReceiver<(String, Invoice)>) {
    make_gateway_with_confirmations(rpc_urls, treasury_address, 0)
}

pub fn make_gateway_with_confirmations(
    rpc_urls: Vec<String>,
    treasury_address: Address,
    min_confirmations: u64,
) -> (PaymentGateway, UnboundedReceiver<(String, Invoice)>) {
    let (tx, rx) = mpsc::unbounded_channel();
    let config = PaymentGatewayConfiguration {
        rpc_urls,
        treasury_address,
        poller_delay_seconds: 0,
        min_confirmations,
        receipt_timeout_seconds: 5,
        sender: tx,
    };
    let gateway = PaymentGateway::new(config).expect("gateway creation must not fail");
    (gateway, rx)
}

/// Convenience wrapper: single-node gateway pointing at `node`, 0 confirmations.
pub fn make_single_node_gateway(
    node: &MockNode,
    treasury_address: Address,
) -> (PaymentGateway, UnboundedReceiver<(String, Invoice)>) {
    make_gateway(vec![node.url.clone()], treasury_address)
}
