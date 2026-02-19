use serde::{Deserialize, Serialize};
use tokio::sync::broadcast;

use super::alerts::Alert;

const CTF_EXCHANGE: &str = "0x4bfb41d5b3570defd03c39a9a4d8de6bd8b8982e";
const NEG_RISK_EXCHANGE: &str = "0xc5d563a36ae78145c45a50134d48a1215220f80a";
const POLL_INTERVAL_SECS: u64 = 4;
const STARTUP_LOOKBACK: u64 = 10;

/// Decode 4-byte function selector to human-readable name.
fn decode_selector(input: &str) -> String {
    if input.len() < 10 {
        return "unknown".into();
    }
    match &input[..10] {
        "0xfc9d554e" => "matchOrders".into(),
        "0x66491c4d" => "fillOrder".into(),
        "0x3cfe1197" => "fillOrders".into(),
        _ => input[..10].to_string(),
    }
}

// ---------------------------------------------------------------------------
// JSON-RPC types
// ---------------------------------------------------------------------------

#[derive(Serialize)]
struct RpcRequest<'a> {
    jsonrpc: &'a str,
    method: &'a str,
    params: serde_json::Value,
    id: u64,
}

#[derive(Deserialize)]
struct RpcResponse<T> {
    result: Option<T>,
    error: Option<RpcErrorValue>,
}

/// eRPC returns `"error": "string"`, standard JSON-RPC returns `"error": {"code":..,"message":..}`
#[derive(Deserialize)]
#[serde(untagged)]
enum RpcErrorValue {
    Str(String),
    Obj { code: i64, message: String },
}

impl std::fmt::Display for RpcErrorValue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Str(s) => f.write_str(s),
            Self::Obj { code, message } => write!(f, "{code}: {message}"),
        }
    }
}

#[derive(Deserialize)]
struct Block {
    #[allow(dead_code)]
    number: Option<String>,
    timestamp: Option<String>,
    #[serde(default)]
    transactions: Vec<Tx>,
}

#[derive(Deserialize)]
struct Tx {
    hash: Option<String>,
    from: Option<String>,
    to: Option<String>,
    input: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Receipt {
    status: Option<String>,
    gas_used: Option<String>,
}

// ---------------------------------------------------------------------------
// JSON-RPC helpers
// ---------------------------------------------------------------------------

async fn rpc_call<T: serde::de::DeserializeOwned>(
    http: &reqwest::Client,
    url: &str,
    method: &str,
    params: serde_json::Value,
) -> Result<T, String> {
    let req = RpcRequest {
        jsonrpc: "2.0",
        method,
        params,
        id: 1,
    };
    let resp = http
        .post(url)
        .json(&req)
        .timeout(std::time::Duration::from_secs(10))
        .send()
        .await
        .map_err(|e| format!("RPC request failed: {e}"))?;

    let text = resp
        .text()
        .await
        .map_err(|e| format!("RPC read body failed: {e}"))?;

    let body: RpcResponse<T> = serde_json::from_str(&text).map_err(|e| {
        let preview = if text.len() > 200 { &text[..200] } else { &text };
        format!("RPC parse failed: {e} — body: {preview}")
    })?;

    if let Some(err) = body.error {
        return Err(format!("RPC error: {err}"));
    }

    body.result.ok_or_else(|| "RPC returned null result".into())
}

async fn get_block_number(http: &reqwest::Client, url: &str) -> Result<u64, String> {
    let hex: String = rpc_call(http, url, "eth_blockNumber", serde_json::json!([])).await?;
    u64::from_str_radix(hex.trim_start_matches("0x"), 16)
        .map_err(|e| format!("Invalid block number: {e}"))
}

async fn get_block(http: &reqwest::Client, url: &str, number: u64) -> Result<Block, String> {
    let hex = format!("0x{number:x}");
    rpc_call(
        http,
        url,
        "eth_getBlockByNumber",
        serde_json::json!([hex, true]),
    )
    .await
}

async fn get_receipt(
    http: &reqwest::Client,
    url: &str,
    tx_hash: &str,
) -> Result<Receipt, String> {
    rpc_call(
        http,
        url,
        "eth_getTransactionReceipt",
        serde_json::json!([tx_hash]),
    )
    .await
}

fn hex_to_u64(hex: &str) -> u64 {
    u64::from_str_radix(hex.trim_start_matches("0x"), 16).unwrap_or(0)
}

// ---------------------------------------------------------------------------
// Main scan loop
// ---------------------------------------------------------------------------

pub async fn run(http: reqwest::Client, rpc_url: String, alert_tx: broadcast::Sender<Alert>) {
    tracing::info!("Phantom fill scanner starting (RPC: {rpc_url})");

    // Wait for RPC to be available
    tokio::time::sleep(std::time::Duration::from_secs(5)).await;

    let mut last_block = loop {
        match get_block_number(&http, &rpc_url).await {
            Ok(n) => break n.saturating_sub(STARTUP_LOOKBACK),
            Err(e) => {
                tracing::warn!("Scanner: waiting for RPC: {e}");
                tokio::time::sleep(std::time::Duration::from_secs(5)).await;
            }
        }
    };

    tracing::info!("Scanner: starting from block {last_block}");
    let mut interval = tokio::time::interval(std::time::Duration::from_secs(POLL_INTERVAL_SECS));

    loop {
        interval.tick().await;

        let head = match get_block_number(&http, &rpc_url).await {
            Ok(n) => n,
            Err(e) => {
                tracing::warn!("Scanner: eth_blockNumber failed: {e}");
                continue;
            }
        };

        // Cap at 20 blocks per cycle to avoid runaway catch-up
        let target = head.min(last_block + 20);

        while last_block < target {
            last_block += 1;
            if let Err(e) = scan_block(&http, &rpc_url, last_block, &alert_tx).await {
                tracing::warn!("Scanner: block {last_block} failed: {e}");
                last_block -= 1;
                break;
            }
        }
    }
}

async fn scan_block(
    http: &reqwest::Client,
    rpc_url: &str,
    block_number: u64,
    alert_tx: &broadcast::Sender<Alert>,
) -> Result<(), String> {
    let block = get_block(http, rpc_url, block_number).await?;

    // Filter TXs targeting exchange contracts
    let exchange_txs: Vec<&Tx> = block
        .transactions
        .iter()
        .filter(|tx| {
            tx.to
                .as_ref()
                .is_some_and(|to| {
                    let lower = to.to_lowercase();
                    lower == CTF_EXCHANGE || lower == NEG_RISK_EXCHANGE
                })
        })
        .collect();

    if exchange_txs.is_empty() {
        return Ok(());
    }

    let block_ts = block.timestamp.as_deref().unwrap_or("0x0");
    let ts_secs = hex_to_u64(block_ts);

    for tx in exchange_txs {
        let tx_hash = tx.hash.as_deref().unwrap_or("");
        let receipt = get_receipt(http, rpc_url, tx_hash).await?;

        // status "0x0" = reverted
        if receipt.status.as_deref() == Some("0x0") {
            let to_lower = tx
                .to
                .as_deref()
                .unwrap_or("")
                .to_lowercase();
            let contract_name = if to_lower == NEG_RISK_EXCHANGE {
                "neg_risk"
            } else {
                "ctf"
            };

            let input = tx.input.as_deref().unwrap_or("");
            let function_name = decode_selector(input);
            let gas_used = hex_to_u64(receipt.gas_used.as_deref().unwrap_or("0x0"));

            tracing::warn!(
                "FAILED SETTLEMENT: tx={tx_hash} block={block_number} from={} contract={contract_name} fn={function_name}",
                tx.from.as_deref().unwrap_or("?")
            );

            let alert = Alert::FailedSettlement {
                tx_hash: tx_hash.into(),
                block_number,
                timestamp: ts_secs.to_string(),
                from_address: tx.from.clone().unwrap_or_default(),
                to_contract: contract_name.into(),
                function_name,
                gas_used: gas_used.to_string(),
            };

            let _ = alert_tx.send(alert);
        }
    }

    Ok(())
}
