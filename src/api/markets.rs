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
    /// CTF condition ID from Gamma API — links to condition_resolution table
    pub condition_id: Option<String>,
    /// Index of this token within the market's clobTokenIds array (maps to payout_numerators)
    pub outcome_index: usize,
}

/// Cache keyed by the first 15 significant digits of the token ID.
/// This handles both full-precision decimal IDs and f64-truncated
/// scientific notation IDs from ClickHouse.
pub type MarketCache = Arc<RwLock<HashMap<String, MarketInfo>>>;

pub fn new_cache() -> MarketCache {
    Arc::new(RwLock::new(HashMap::new()))
}

/// Convert scientific notation to an integer string (no-op for already-integer IDs).
/// "4.366244298967411e75" → "43662442989674110000..." (lossy but displayable)
/// "51797304566750985981..." → "51797304566750985981..." (no-op)
/// Only needed for legacy trades stored before the UInt256 migration.
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
pub(crate) fn cache_key(token_id: &str) -> String {
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
                                    condition_id: market.condition_id.clone(),
                                    outcome_index: i,
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

/// Cross-reference the warm cache with on-chain ConditionResolution events,
/// compute exact resolved prices, and write them to the resolved_prices table.
pub async fn populate_resolved_prices(db: &clickhouse::Client, cache: &MarketCache) {
    use super::types::{ConditionResolutionRow, ResolvedPriceRow};

    // 1. Query all condition resolutions from ClickHouse
    let resolutions: Vec<ConditionResolutionRow> = match db
        .query(
            "SELECT condition_id, payout_numerators, block_number
             FROM poly_dearboard_conditional_tokens.condition_resolution",
        )
        .fetch_all()
        .await
    {
        Ok(rows) => rows,
        Err(e) => {
            tracing::warn!("Failed to query condition_resolution: {e}");
            return;
        }
    };

    if resolutions.is_empty() {
        tracing::info!("No condition resolutions found, skipping resolved prices");
        return;
    }

    // 2. Build condition_id → (payout_numerators, block_number) map
    let resolution_map: HashMap<String, (&Vec<String>, u64)> = resolutions
        .iter()
        .map(|r| (r.condition_id.clone(), (&r.payout_numerators, r.block_number)))
        .collect();

    tracing::info!(
        "Found {} on-chain condition resolutions",
        resolution_map.len()
    );

    // 3. Query distinct ClickHouse asset_ids
    let ch_assets: Vec<AssetIdRow> = match db
        .query("SELECT DISTINCT asset_id FROM poly_dearboard.trades")
        .fetch_all()
        .await
    {
        Ok(rows) => rows,
        Err(e) => {
            tracing::warn!("Failed to query asset_ids for resolved prices: {e}");
            return;
        }
    };

    // 4. For each CH asset_id, look up cache entry → check resolution → compute price
    let cache_read = cache.read().await;
    let mut rows: Vec<ResolvedPriceRow> = Vec::new();

    for asset in &ch_assets {
        let key = cache_key(&asset.asset_id);
        let info = match cache_read.get(&key) {
            Some(i) => i,
            None => continue,
        };
        let cid = match &info.condition_id {
            Some(c) => c,
            None => continue,
        };
        // On-chain condition_id has no 0x prefix; Gamma stores it with 0x — strip for lookup
        let bare_cid = cid.strip_prefix("0x").unwrap_or(cid);
        let (numerators, block) = match resolution_map.get(bare_cid) {
            Some(r) => r,
            None => continue, // Not resolved on-chain
        };

        // resolved_price = numerators[outcome_index] / sum(numerators)
        let nums: Vec<f64> = numerators.iter().filter_map(|s| s.parse().ok()).collect();
        let total: f64 = nums.iter().sum();
        if total <= 0.0 || info.outcome_index >= nums.len() {
            continue;
        }
        let price = nums[info.outcome_index] / total;

        rows.push(ResolvedPriceRow {
            asset_id: asset.asset_id.clone(),
            resolved_price: format!("{:.6}", price),
            condition_id: cid.clone(),
            block_number: *block,
        });
    }

    drop(cache_read);

    if rows.is_empty() {
        tracing::info!("No resolved prices to populate (no cache↔resolution overlap)");
        return;
    }

    // 5. Truncate + batch INSERT
    if let Err(e) = db
        .query("TRUNCATE TABLE IF EXISTS poly_dearboard.resolved_prices")
        .execute()
        .await
    {
        tracing::warn!("Failed to truncate resolved_prices: {e}");
    }

    let mut inserter = match db.insert("poly_dearboard.resolved_prices") {
        Ok(i) => i,
        Err(e) => {
            tracing::warn!("Failed to create inserter for resolved_prices: {e}");
            return;
        }
    };

    let count = rows.len();
    for row in rows {
        if let Err(e) = inserter.write(&row).await {
            tracing::warn!("Failed to write resolved_price row: {e}");
            return;
        }
    }

    if let Err(e) = inserter.end().await {
        tracing::warn!("Failed to flush resolved_prices: {e}");
        return;
    }

    tracing::info!("Populated {count} resolved prices from on-chain data");
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
    // After UInt256 migration, token_id is a full-precision integer string.
    // For legacy scientific notation IDs, convert to integer form for Gamma lookup.
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
    let matched_idx = ids.iter().position(|id| id == &lookup_id);
    let outcome = matched_idx
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
        condition_id: market.condition_id,
        outcome_index: matched_idx.unwrap_or(0),
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
    /// CTF condition ID — links to on-chain ConditionResolution events
    condition_id: Option<String>,
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
