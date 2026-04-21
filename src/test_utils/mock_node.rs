/// In-process mock Ethereum JSON-RPC node backed by an in-memory state machine.
///
/// Serves just the `eth_*` methods that `acceptevm` calls, making integration
/// tests fully self-contained with no external Anvil/Hardhat process needed.
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use alloy::consensus::TxEnvelope;
use alloy::eips::eip2718::Decodable2718;
use alloy::primitives::{keccak256, Address, B256, U256};
use axum::extract::State;
use axum::routing::post;
use axum::{Json, Router};
use serde_json::{json, Value};
use tokio::sync::oneshot;

// ─── Receipt ─────────────────────────────────────────────────────────────────

#[derive(Clone, Debug)]
pub struct MockReceipt {
    pub block_number: u64,
    pub from: Address,
    pub to: Address,
    pub status: bool,
}

// ─── State ───────────────────────────────────────────────────────────────────

pub struct MockEvmState {
    pub balances: HashMap<Address, U256>,
    pub nonces: HashMap<Address, u64>,
    /// tx_hash → receipt
    pub receipts: HashMap<B256, MockReceipt>,
    pub block_number: u64,
    pub chain_id: u64,
    /// If set, the receipt for this hash will be withheld on the *first* fetch
    /// only (simulates a receipt disappearing after a reorg).
    pub drop_receipt_once: Option<B256>,
    /// Counters so tests can verify round-robin behaviour.
    pub request_count: u64,
}

impl MockEvmState {
    pub fn new(chain_id: u64) -> Self {
        Self {
            balances: HashMap::new(),
            nonces: HashMap::new(),
            receipts: HashMap::new(),
            block_number: 1,
            chain_id,
            drop_receipt_once: None,
            request_count: 0,
        }
    }
}

// ─── Node handle ─────────────────────────────────────────────────────────────

#[derive(Clone)]
pub struct MockNode {
    pub state: Arc<Mutex<MockEvmState>>,
    pub url: String,
    _shutdown_tx: Arc<Mutex<Option<oneshot::Sender<()>>>>,
}

impl MockNode {
    /// Spin up a mock node on a random OS-assigned port and return a handle.
    /// Must be called from within a tokio async context.
    pub async fn start() -> Self {
        Self::start_with_chain_id(1).await
    }

    pub async fn start_with_chain_id(chain_id: u64) -> Self {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("failed to bind mock node");
        let port = listener.local_addr().unwrap().port();
        let url = format!("http://127.0.0.1:{port}");

        let state = Arc::new(Mutex::new(MockEvmState::new(chain_id)));
        let (shutdown_tx, shutdown_rx) = oneshot::channel::<()>();

        let app_state = state.clone();
        let app = Router::new()
            .route("/", post(handle_rpc))
            .with_state(app_state);

        tokio::spawn(async move {
            axum::serve(listener, app)
                .with_graceful_shutdown(async move {
                    let _ = shutdown_rx.await;
                })
                .await
                .ok();
        });

        MockNode {
            state,
            url,
            _shutdown_tx: Arc::new(Mutex::new(Some(shutdown_tx))),
        }
    }

    // ── State helpers ─────────────────────────────────────────────────────────

    pub fn set_balance(&self, addr: Address, balance: U256) {
        self.state.lock().unwrap().balances.insert(addr, balance);
    }

    pub fn get_balance(&self, addr: Address) -> U256 {
        self.state
            .lock()
            .unwrap()
            .balances
            .get(&addr)
            .cloned()
            .unwrap_or(U256::ZERO)
    }

    pub fn get_treasury_balance(&self, addr: Address) -> U256 {
        self.get_balance(addr)
    }

    pub fn mine_blocks(&self, n: u64) {
        self.state.lock().unwrap().block_number += n;
    }

    pub fn block_number(&self) -> u64 {
        self.state.lock().unwrap().block_number
    }

    pub fn request_count(&self) -> u64 {
        self.state.lock().unwrap().request_count
    }

    /// Cause the receipt for `hash` to be withheld on the very next fetch.
    pub fn drop_receipt_once(&self, hash: B256) {
        self.state.lock().unwrap().drop_receipt_once = Some(hash);
    }

    /// Returns the first pending tx hash that was stored (any receipt).
    pub fn any_tx_hash(&self) -> Option<B256> {
        self.state
            .lock()
            .unwrap()
            .receipts
            .keys()
            .next()
            .cloned()
    }
}

// ─── JSON-RPC handler ─────────────────────────────────────────────────────────

type AppState = Arc<Mutex<MockEvmState>>;

async fn handle_rpc(
    State(state): State<AppState>,
    Json(body): Json<Value>,
) -> Json<Value> {
    let id = body.get("id").cloned().unwrap_or(json!(1));
    let method = body
        .get("method")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let params = body
        .get("params")
        .cloned()
        .unwrap_or(json!([]));

    {
        let mut s = state.lock().unwrap();
        s.request_count += 1;
    }

    let result = dispatch(&state, method, &params).await;

    match result {
        Ok(val) => Json(json!({ "jsonrpc": "2.0", "id": id, "result": val })),
        Err(msg) => Json(json!({
            "jsonrpc": "2.0",
            "id": id,
            "error": { "code": -32000, "message": msg }
        })),
    }
}

async fn dispatch(
    state: &AppState,
    method: &str,
    params: &Value,
) -> Result<Value, String> {
    match method {
        // ── Chain metadata ────────────────────────────────────────────────────

        "eth_chainId" => {
            let chain_id = state.lock().unwrap().chain_id;
            Ok(json!(format!("{:#x}", chain_id)))
        }

        "eth_blockNumber" => {
            let bn = state.lock().unwrap().block_number;
            Ok(json!(format!("{:#x}", bn)))
        }

        // ── Balance / nonce ───────────────────────────────────────────────────

        "eth_getBalance" => {
            let addr = parse_address(params, 0)?;
            let bal = state
                .lock()
                .unwrap()
                .balances
                .get(&addr)
                .cloned()
                .unwrap_or(U256::ZERO);
            Ok(json!(format!("{:#x}", bal)))
        }

        "eth_getTransactionCount" => {
            let addr = parse_address(params, 0)?;
            let nonce = state
                .lock()
                .unwrap()
                .nonces
                .get(&addr)
                .cloned()
                .unwrap_or(0);
            Ok(json!(format!("{:#x}", nonce)))
        }

        // ── Gas ───────────────────────────────────────────────────────────────

        "eth_gasPrice" => {
            // 1 gwei
            Ok(json!("0x3b9aca00"))
        }

        "eth_estimateGas" => {
            // Standard native transfer
            Ok(json!("0x5208"))
        }

        // Reject EIP-1559 estimation so alloy falls back to legacy gas price.
        "eth_feeHistory" | "eth_maxPriorityFeePerGas" => {
            Err("not supported by mock node".to_string())
        }

        // ── Send transaction ──────────────────────────────────────────────────

        "eth_sendRawTransaction" => {
            let raw_hex = params
                .get(0)
                .and_then(|v| v.as_str())
                .ok_or("missing raw tx param")?;

            let raw_bytes = decode_hex(raw_hex)?;
            let tx_hash = B256::from(keccak256(&raw_bytes));

            // Decode + recover sender
            let mut buf = raw_bytes.as_slice();
            let tx = TxEnvelope::decode_2718(&mut buf)
                .map_err(|e| format!("tx decode error: {e}"))?;

            // Recover sender by matching on each typed-tx variant
            let sender = match &tx {
                TxEnvelope::Legacy(s)  => s.recover_signer(),
                TxEnvelope::Eip2930(s) => s.recover_signer(),
                TxEnvelope::Eip1559(s) => s.recover_signer(),
                TxEnvelope::Eip4844(s) => s.recover_signer(),
                TxEnvelope::Eip7702(s) => s.recover_signer(),
                _ => return Err("unknown transaction type".to_string()),
            }
            .map_err(|e| format!("signer recovery error: {e}"))?;

            use alloy::consensus::Transaction as _;
            let value = tx.value();
            let gas_limit = tx.gas_limit();
            let gas_price = tx.gas_price().unwrap_or_else(|| tx.max_fee_per_gas());
            // In alloy 2.0 Transaction::to() returns Option<Address>
            let to_addr = tx
                .to()
                .ok_or_else(|| "contract deployment not supported".to_string())?;

            let gas_cost = U256::from(gas_limit) * U256::from(gas_price);

            // Mutate state: deduct from sender, credit recipient
            {
                let mut s = state.lock().unwrap();
                let sender_bal = s.balances.entry(sender).or_insert(U256::ZERO);
                *sender_bal = sender_bal.saturating_sub(value).saturating_sub(gas_cost);

                *s.balances.entry(to_addr).or_insert(U256::ZERO) += value;

                let nonce = s.nonces.entry(sender).or_insert(0);
                *nonce += 1;

                // Store receipt at the *current* block
                let block_number = s.block_number;
                s.receipts.insert(
                    tx_hash,
                    MockReceipt {
                        block_number,
                        from: sender,
                        to: to_addr,
                        status: true,
                    },
                );
            }

            Ok(json!(format!("{:#x}", tx_hash)))
        }

        // ── Receipt ───────────────────────────────────────────────────────────

        "eth_getTransactionReceipt" => {
            let hash = parse_b256(params, 0)?;

            let mut s = state.lock().unwrap();

            // Fault injection: withhold receipt once for reorg simulation
            if s.drop_receipt_once == Some(hash) {
                s.drop_receipt_once = None;
                return Ok(Value::Null);
            }

            match s.receipts.get(&hash) {
                None => Ok(Value::Null),
                Some(r) => {
                    // logsBloom must be exactly 256 bytes = 512 hex chars + "0x"
                    let bloom = format!("0x{}", "0".repeat(512));
                    Ok(json!({
                        "transactionHash": format!("{:#x}", hash),
                        "blockNumber": format!("{:#x}", r.block_number),
                        "blockHash": format!("{:#x}", hash), // reuse hash as dummy block hash
                        "transactionIndex": "0x0",
                        "from": format!("{:#x}", r.from),
                        "to": format!("{:#x}", r.to),
                        "contractAddress": Value::Null,
                        "status": if r.status { "0x1" } else { "0x0" },
                        "gasUsed": "0x5208",
                        "cumulativeGasUsed": "0x5208",
                        "effectiveGasPrice": "0x3b9aca00",
                        "logs": [],
                        "logsBloom": bloom,
                        "type": "0x2"
                    }))
                }
            }
        }

        // ── Net ───────────────────────────────────────────────────────────────

        "net_version" => {
            let chain_id = state.lock().unwrap().chain_id;
            Ok(json!(chain_id.to_string()))
        }

        other => Err(format!("method not supported by mock node: {other}")),
    }
}

// ─── Helpers ──────────────────────────────────────────────────────────────────

fn parse_address(params: &Value, idx: usize) -> Result<Address, String> {
    params
        .get(idx)
        .and_then(|v| v.as_str())
        .ok_or_else(|| "missing address param".to_string())?
        .parse::<Address>()
        .map_err(|e| format!("invalid address: {e}"))
}

fn parse_b256(params: &Value, idx: usize) -> Result<B256, String> {
    params
        .get(idx)
        .and_then(|v| v.as_str())
        .ok_or_else(|| "missing hash param".to_string())?
        .parse::<B256>()
        .map_err(|e| format!("invalid hash: {e}"))
}

fn decode_hex(s: &str) -> Result<Vec<u8>, String> {
    let s = s.strip_prefix("0x").unwrap_or(s);
    hex::decode(s).map_err(|e| format!("invalid hex: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloy::primitives::U256;

    #[tokio::test]
    async fn mock_node_responds_to_chain_id() {
        let node = MockNode::start().await;

        // Send a raw JSON-RPC request via reqwest
        let client = reqwest::Client::new();
        let resp: serde_json::Value = client
            .post(&node.url)
            .json(&serde_json::json!({
                "jsonrpc": "2.0",
                "id": 1,
                "method": "eth_chainId",
                "params": []
            }))
            .send()
            .await
            .expect("request must succeed")
            .json()
            .await
            .expect("response must be valid JSON");

        assert!(resp.get("result").is_some(), "eth_chainId must return a result");
    }

    #[tokio::test]
    async fn mock_node_balance_set_and_get() {
        let node = MockNode::start().await;
        let addr = Address::repeat_byte(0xAB);
        let expected = U256::from(12345u64);
        node.set_balance(addr, expected);

        let client = reqwest::Client::new();
        let resp: serde_json::Value = client
            .post(&node.url)
            .json(&serde_json::json!({
                "jsonrpc": "2.0",
                "id": 1,
                "method": "eth_getBalance",
                "params": [format!("{addr:#x}"), "latest"]
            }))
            .send()
            .await
            .expect("request must succeed")
            .json()
            .await
            .expect("valid JSON");

        let result_hex = resp["result"].as_str().expect("result must be a string");
        let returned = U256::from_str_radix(result_hex.trim_start_matches("0x"), 16)
            .expect("valid balance hex");
        assert_eq!(returned, expected);
    }

    /// Full gateway pipeline smoke test — verifies that a funded invoice
    /// actually triggers the confirmation callback when the mock node is used.
    #[tokio::test]
    async fn gateway_confirms_funded_invoice_via_mock_node() {
        use crate::test_utils::gateway_helpers::make_single_node_gateway;
        use crate::gateway::U256;
        use std::time::Duration;
        use tokio::time::timeout;
        use alloy::providers::{Provider, ProviderBuilder};

        let node = MockNode::start().await;
        let treasury = Address::repeat_byte(0xAB);
        let (gateway, mut rx) = make_single_node_gateway(&node, treasury);

        let amount = U256::from(1_000_000_000_000_000_000u128); // 1 ETH
        let (_, invoice) = gateway
            .new_invoice(amount, vec![], 3600)
            .await
            .expect("invoice creation must succeed");

        node.set_balance(invoice.to, amount);

        // ── Step 1: verify the mock responds to eth_getBalance directly
        let url: reqwest::Url = node.url.parse().expect("url");
        let provider = ProviderBuilder::new().connect_http(url);
        let bal = provider.get_balance(invoice.to).await.expect("balance fetch must succeed");
        eprintln!("[smoke] direct balance check: {bal}");
        assert_eq!(bal, amount, "direct balance check must equal amount");

        // ── Step 2: start the poller and wait for confirmation
        gateway.poll_payments().await;

        let result = timeout(Duration::from_secs(15), rx.recv()).await;
        match &result {
            Ok(Some((id, inv))) => {
                eprintln!("[smoke] confirmed id={id}, hash={:?}", inv.hash);
            }
            Ok(None) => eprintln!("[smoke] channel closed"),
            Err(_) => {
                // Log node request count to see if it was even contacted
                eprintln!("[smoke] timed out; node request count={}", node.request_count());
                // Check if a treasury tx was submitted
                let state = node.state.lock().unwrap();
                eprintln!("[smoke] node receipts count={}", state.receipts.len());
                for (h, r) in &state.receipts {
                    eprintln!("[smoke]   receipt hash={h:#x} block={}", r.block_number);
                }
                drop(state);
                
                // Try to manually confirm via alloy
                if let Some(hash) = node.any_tx_hash() {
                    let hash_str = format!("{hash:#x}");
                    eprintln!("[smoke] receipt hash stored: {hash_str}");

                    // Call eth_getTransactionReceipt directly via alloy
                    let url2: reqwest::Url = node.url.parse().unwrap();
                    let prov2 = ProviderBuilder::new().connect_http(url2);
                    match prov2.get_transaction_receipt(hash).await {
                        Ok(Some(r)) => eprintln!("[smoke] receipt block_number={:?}", r.block_number),
                        Ok(None) => eprintln!("[smoke] receipt not found"),
                        Err(e) => eprintln!("[smoke] receipt error: {e}"),
                    }

                    // Also check block number
                    match prov2.get_block_number().await {
                        Ok(bn) => eprintln!("[smoke] latest_block_number={bn}"),
                        Err(e) => eprintln!("[smoke] block number error: {e}"),
                    }
                }
            }
        }
        assert!(result.is_ok(), "gateway must confirm funded invoice");
    }
}
