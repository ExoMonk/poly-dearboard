use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use tokio::sync::RwLock;

const PREFIX_LEN: usize = 15;

#[derive(Clone, Debug)]
pub struct MarketInfo {
    pub question: String,
    pub outcome: String,
    pub category: String,
    pub active: bool,
    /// Full-precision token ID from Gamma API (for lookups that need the exact uint256)
    pub gamma_token_id: String,
}

/// Cache keyed by the first 15 significant digits of the token ID.
/// This handles both full-precision decimal IDs and f64-truncated
/// scientific notation IDs from ClickHouse.
pub type MarketCache = Arc<RwLock<HashMap<String, MarketInfo>>>;

pub fn new_cache() -> MarketCache {
    Arc::new(RwLock::new(HashMap::new()))
}

/// Convert any token ID to ClickHouse's stored format (f64 scientific notation).
/// "43662442989674113827..." → "4.366244298967411e75"  (full-precision → scientific)
/// "4.366244298967411e75"   → "4.366244298967411e75"  (already scientific, no-op)
/// "0"                      → "0"                      (small values unchanged)
pub fn to_clickhouse_id(id: &str) -> String {
    if let Ok(f) = id.parse::<f64>() {
        if f > 1e15 && f.is_finite() {
            return format!("{:e}", f);
        }
    }
    id.to_string()
}

/// Convert any token ID to an integer string (no scientific notation).
/// "4.366244298967411e75" → "43662442989674110000..." (lossy but usable for display/API)
/// "51797304566750985981..." → "51797304566750985981..." (already integer, no-op)
/// Used as fallback when gamma_token_id is unavailable.
pub fn to_integer_id(id: &str) -> String {
    if id.contains('e') || id.contains('E') {
        if let Ok(f) = id.parse::<f64>() {
            if f.is_finite() {
                return format!("{:.0}", f);
            }
        }
    }
    id.to_string()
}

/// Extract the significant digits from a token ID string.
/// "8.715511933644157e75" → "8715511933644157"
/// "51797304566750985981..." → "51797304566750985981..."
fn significant_digits(id: &str) -> String {
    let e_pos = match id.find('e').or_else(|| id.find('E')) {
        Some(pos) => pos,
        None => return id.to_string(),
    };
    let mantissa = &id[..e_pos];
    mantissa.replace('.', "")
}

/// Build a cache key: first 15 significant digits.
fn cache_key(token_id: &str) -> String {
    let sig = significant_digits(token_id);
    if sig.len() >= PREFIX_LEN {
        sig[..PREFIX_LEN].to_string()
    } else {
        sig
    }
}

/// Pre-warm the cache by fetching Gamma events targeted to tokens in ClickHouse.
/// Queries ClickHouse for all distinct asset_ids, then paginates Gamma events
/// until every ClickHouse token has a full-precision match (or pagination exhausted).
pub async fn warm_cache(http: &reqwest::Client, db: &clickhouse::Client, cache: &MarketCache) {
    // 1. Get all distinct token prefixes from ClickHouse
    let target_prefixes: HashSet<String> = match db
        .query("SELECT DISTINCT asset_id FROM poly_dearboard.trades")
        .fetch_all::<AssetIdRow>()
        .await
    {
        Ok(rows) => rows.iter().map(|r| cache_key(&r.asset_id)).collect(),
        Err(e) => {
            tracing::warn!("Failed to query ClickHouse for asset_ids: {e}");
            return;
        }
    };

    if target_prefixes.is_empty() {
        tracing::info!("No tokens in ClickHouse, skipping warm cache");
        return;
    }

    let target_count = target_prefixes.len();
    tracing::info!("Warming cache for {target_count} distinct ClickHouse tokens...");

    // 2. Paginate Gamma events, caching only tokens that match ClickHouse prefixes
    let mut covered: HashSet<String> = HashSet::new();
    let mut offset = 0u32;
    let batch = 100u32;
    let max_offset = 100_000u32;

    loop {
        let url = format!(
            "https://gamma-api.polymarket.com/events?limit={batch}&offset={offset}&order=volume24hr&ascending=false"
        );

        let resp = match http
            .get(&url)
            .timeout(std::time::Duration::from_secs(15))
            .send()
            .await
        {
            Ok(r) => r,
            Err(e) => {
                tracing::warn!("Market cache warm failed at offset {offset}: {e}");
                break;
            }
        };

        let events: Vec<GammaEvent> = match resp.json().await {
            Ok(e) => e,
            Err(e) => {
                tracing::warn!("Market cache parse failed at offset {offset}: {e}");
                break;
            }
        };

        let count = events.len();

        {
            let mut c = cache.write().await;
            for event in &events {
                let category = event.first_tag();
                for market in &event.markets {
                    let ids = market.parsed_token_ids();
                    let outcomes = market.parsed_outcomes();
                    let active = market.is_active();
                    for (i, id) in ids.iter().enumerate() {
                        let key = cache_key(id);
                        if target_prefixes.contains(&key) {
                            let outcome = outcomes.get(i).cloned().unwrap_or_default();
                            c.insert(
                                key.clone(),
                                MarketInfo {
                                    question: market.question.clone().unwrap_or_default(),
                                    outcome,
                                    category: category.clone(),
                                    active,
                                    gamma_token_id: id.clone(),
                                },
                            );
                            covered.insert(key);
                        }
                    }
                }
            }
        }

        if covered.len() >= target_count {
            break;
        }
        if count < batch as usize {
            break;
        }
        offset += batch;
        if offset >= max_offset {
            break;
        }

        if offset % 5000 == 0 {
            tracing::info!(
                "Warm cache progress: {}/{} tokens covered ({offset} events scanned)",
                covered.len(),
                target_count
            );
        }
    }

    tracing::info!(
        "Warmed market cache: {}/{} ClickHouse tokens covered ({offset} events scanned)",
        covered.len(),
        target_count
    );
}

#[derive(clickhouse::Row, serde::Deserialize)]
struct AssetIdRow {
    asset_id: String,
}

/// Resolve token IDs to market info.
///
/// Lookup strategy:
/// 1. Prefix match against the pre-warmed cache (handles f64 precision loss)
/// 2. For cache misses with full-precision IDs, try individual Gamma API calls
pub async fn resolve_markets(
    http: &reqwest::Client,
    cache: &MarketCache,
    token_ids: &[String],
) -> HashMap<String, MarketInfo> {
    let mut result = HashMap::new();
    let mut uncached: Vec<String> = Vec::new();

    {
        let c = cache.read().await;
        for id in token_ids {
            let key = cache_key(id);
            if let Some(info) = c.get(&key) {
                result.insert(id.clone(), info.clone());
            } else {
                uncached.push(id.clone());
            }
        }
    }

    if uncached.is_empty() {
        return result;
    }

    // Resolve uncached full-precision IDs via Gamma API (max 10 concurrent)
    let sem = Arc::new(tokio::sync::Semaphore::new(10));
    let mut handles = Vec::new();

    for id in &uncached {
        let http = http.clone();
        let id = id.clone();
        let permit = Arc::clone(&sem).acquire_owned().await.unwrap();

        handles.push(tokio::spawn(async move {
            let _permit = permit;
            fetch_market_info(&http, &id).await
        }));
    }

    let mut new_entries = Vec::new();
    for (i, handle) in handles.into_iter().enumerate() {
        if let Ok(Some(info)) = handle.await {
            new_entries.push((uncached[i].clone(), info));
        }
    }

    if !new_entries.is_empty() {
        let mut c = cache.write().await;
        for (id, info) in &new_entries {
            c.insert(cache_key(id), info.clone());
            result.insert(id.clone(), info.clone());
        }
    }

    result
}

async fn fetch_market_info(http: &reqwest::Client, token_id: &str) -> Option<MarketInfo> {
    // Gamma API requires integer token IDs — never scientific notation.
    // Convert scientific notation to integer form (lossy but the API needs a plain number).
    let lookup_id = to_integer_id(token_id);

    let url = format!(
        "https://gamma-api.polymarket.com/markets?clob_token_ids={}",
        lookup_id
    );

    let resp = http
        .get(&url)
        .timeout(std::time::Duration::from_secs(5))
        .send()
        .await
        .ok()?;

    let markets: Vec<GammaMarket> = resp.json().await.ok()?;
    let market = markets.into_iter().next()?;

    let ids = market.parsed_token_ids();
    let outcomes = market.parsed_outcomes();
    // Match by prefix since the lookup_id may be lossy
    let outcome = ids
        .iter()
        .position(|id| id == &lookup_id || cache_key(id) == cache_key(token_id))
        .and_then(|idx| outcomes.get(idx).cloned())
        .unwrap_or_default();

    let gamma_token_id = ids
        .iter()
        .find(|id| cache_key(id) == cache_key(token_id))
        .cloned()
        .unwrap_or_else(|| lookup_id);

    let active = market.is_active();
    Some(MarketInfo {
        question: market.question.unwrap_or_default(),
        outcome,
        category: String::new(),
        active,
        gamma_token_id,
    })
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct GammaEvent {
    markets: Vec<GammaMarket>,
    #[serde(default)]
    tags: Vec<GammaTag>,
}

impl GammaEvent {
    fn first_tag(&self) -> String {
        self.tags
            .iter()
            .map(|t| t.label.as_str())
            .find(|l| *l != "Parent For Derivative")
            .unwrap_or("")
            .to_string()
    }
}

#[derive(serde::Deserialize)]
struct GammaTag {
    label: String,
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct GammaMarket {
    question: Option<String>,
    /// JSON-encoded string array, e.g. "[\"Yes\", \"No\"]"
    outcomes: Option<String>,
    /// JSON-encoded string array of token IDs
    clob_token_ids: Option<String>,
    #[serde(default)]
    active: Option<bool>,
    #[serde(default)]
    closed: Option<bool>,
}

impl GammaMarket {
    fn is_active(&self) -> bool {
        // Market is active if not explicitly closed and not explicitly inactive
        !self.closed.unwrap_or(false) && self.active.unwrap_or(true)
    }

    fn parsed_outcomes(&self) -> Vec<String> {
        self.outcomes
            .as_deref()
            .and_then(|s| serde_json::from_str(s).ok())
            .unwrap_or_default()
    }

    fn parsed_token_ids(&self) -> Vec<String> {
        self.clob_token_ids
            .as_deref()
            .and_then(|s| serde_json::from_str(s).ok())
            .unwrap_or_default()
    }
}
