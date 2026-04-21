/// Verifies that an invoice whose wallet bytes are corrupted (not a valid
/// secp256k1 key) causes a logged error but does NOT crash the poller,
/// and the invoice is never falsely confirmed.
use std::time::Duration;

use alloy::primitives::{Address, U256};
use tokio::time::timeout;

use crate::invoice::{Invoice, ZeroizedVec};
use crate::gateway::get_unix_time_seconds;
use crate::test_utils::{gateway_helpers::make_single_node_gateway, mock_node::MockNode};

const TREASURY: Address = Address::repeat_byte(0x66);

#[tokio::test]
async fn test_corrupted_wallet_does_not_crash_poller() {
    let node = MockNode::start().await;
    let (gateway, mut rx) = make_single_node_gateway(&node, TREASURY);

    // Craft a fake invoice with invalid wallet bytes (only 5 bytes, not 32)
    let bad_wallet = ZeroizedVec { inner: vec![0xDE, 0xAD, 0xBE, 0xEF, 0x00] };
    let fake_address = Address::repeat_byte(0x77);
    let amount = U256::from(1_000_000_000_000_000_000u128);

    let bad_invoice = Invoice {
        to: fake_address,
        wallet: bad_wallet,
        amount,
        message: vec![],
        expires: get_unix_time_seconds() + 3600,
        paid_at_timestamp: 0,
        hash: None,
        nonce: None,
    };

    // Inject the bad invoice directly into the gateway's invoice map
    {
        let mut map = gateway.invoices.write().await;
        map.insert("bad-invoice-key".to_string(), bad_invoice);
    }

    // Fund the bad address so the poller tries to sweep (and fails)
    node.set_balance(fake_address, amount);

    gateway.poll_payments().await;

    // Poller must NOT confirm or crash — expect silence for 3 seconds
    let result = timeout(Duration::from_secs(3), rx.recv()).await;
    assert!(
        result.is_err(),
        "corrupted-wallet invoice must never be confirmed"
    );
}

#[tokio::test]
async fn test_valid_invoice_after_bad_one_still_confirms() {
    let node = MockNode::start().await;
    let (gateway, mut rx) = make_single_node_gateway(&node, TREASURY);

    // Inject a bad invoice
    let bad_wallet = ZeroizedVec { inner: vec![0xFF; 10] }; // wrong length
    let fake_addr = Address::repeat_byte(0x88);
    let amount = U256::from(1_000_000_000_000_000_000u128);

    let bad_invoice = Invoice {
        to: fake_addr,
        wallet: bad_wallet,
        amount,
        message: vec![],
        expires: get_unix_time_seconds() + 3600,
        paid_at_timestamp: 0,
        hash: None,
        nonce: None,
    };
    {
        let mut map = gateway.invoices.write().await;
        map.insert("bad-key".to_string(), bad_invoice);
    }
    node.set_balance(fake_addr, amount);

    // Also create a valid invoice
    let (good_id, good_inv) = gateway
        .new_invoice(amount, b"good".to_vec(), 3600)
        .await
        .expect("invoice creation must succeed");
    node.set_balance(good_inv.to, amount);

    gateway.poll_payments().await;

    // The valid invoice must still confirm despite the bad one in the map
    let (confirmed_id, _) = timeout(Duration::from_secs(15), rx.recv())
        .await
        .expect("valid invoice must confirm even when a bad invoice is present")
        .expect("channel closed");

    assert_eq!(confirmed_id, good_id);
}
