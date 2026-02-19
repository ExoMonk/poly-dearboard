-- Poly-Dearboard ClickHouse Schema
-- Pre-creates rindexer raw event tables + normalized trades table + materialized views.
-- rindexer's CREATE TABLE IF NOT EXISTS will safely skip tables that already exist.
--
-- NOTE: ClickHouse does NOT support UNION ALL in materialized views.
-- Each MV has a single SELECT; multiple MVs write to the same target table.

-- =============================================================================
-- 1. rindexer raw event databases and tables
-- =============================================================================

-- CTF Exchange
CREATE DATABASE IF NOT EXISTS poly_dearboard_ctf_exchange;

CREATE TABLE IF NOT EXISTS poly_dearboard_ctf_exchange.order_filled (
    contract_address  FixedString(42),
    order_hash        String,
    maker             FixedString(42),
    taker             FixedString(42),
    maker_asset_id    String,
    taker_asset_id    String,
    maker_amount_filled String,
    taker_amount_filled String,
    fee               String,
    tx_hash           FixedString(66),
    block_number      UInt64,
    block_timestamp   Nullable(DateTime('UTC')),
    block_hash        FixedString(66),
    network           String,
    tx_index          UInt64,
    log_index         UInt64,

    INDEX idx_block_num (block_number) TYPE minmax GRANULARITY 1,
    INDEX idx_timestamp (block_timestamp) TYPE bloom_filter GRANULARITY 1,
    INDEX idx_network (network) TYPE bloom_filter GRANULARITY 1,
    INDEX idx_tx_hash (tx_hash) TYPE bloom_filter GRANULARITY 1
) ENGINE = ReplacingMergeTree
ORDER BY (network, block_number, tx_hash, log_index)
TTL ifNull(block_timestamp, toDateTime('1970-01-01')) + INTERVAL 1 DAY;

-- NegRisk CTF Exchange
CREATE DATABASE IF NOT EXISTS poly_dearboard_neg_risk_ctf_exchange;

CREATE TABLE IF NOT EXISTS poly_dearboard_neg_risk_ctf_exchange.order_filled (
    contract_address  FixedString(42),
    order_hash        String,
    maker             FixedString(42),
    taker             FixedString(42),
    maker_asset_id    String,
    taker_asset_id    String,
    maker_amount_filled String,
    taker_amount_filled String,
    fee               String,
    tx_hash           FixedString(66),
    block_number      UInt64,
    block_timestamp   Nullable(DateTime('UTC')),
    block_hash        FixedString(66),
    network           String,
    tx_index          UInt64,
    log_index         UInt64,

    INDEX idx_block_num (block_number) TYPE minmax GRANULARITY 1,
    INDEX idx_timestamp (block_timestamp) TYPE bloom_filter GRANULARITY 1,
    INDEX idx_network (network) TYPE bloom_filter GRANULARITY 1,
    INDEX idx_tx_hash (tx_hash) TYPE bloom_filter GRANULARITY 1
) ENGINE = ReplacingMergeTree
ORDER BY (network, block_number, tx_hash, log_index)
TTL ifNull(block_timestamp, toDateTime('1970-01-01')) + INTERVAL 1 DAY;

-- Conditional Tokens
CREATE DATABASE IF NOT EXISTS poly_dearboard_conditional_tokens;

CREATE TABLE IF NOT EXISTS poly_dearboard_conditional_tokens.payout_redemption (
    contract_address  FixedString(42),
    redeemer          FixedString(42),
    collateral_token  FixedString(42),
    parent_collection_id String,
    condition_id      String,
    index_sets        Array(String),
    payout            String,
    tx_hash           FixedString(66),
    block_number      UInt64,
    block_timestamp   Nullable(DateTime('UTC')),
    block_hash        FixedString(66),
    network           String,
    tx_index          UInt64,
    log_index         UInt64,

    INDEX idx_block_num (block_number) TYPE minmax GRANULARITY 1,
    INDEX idx_timestamp (block_timestamp) TYPE bloom_filter GRANULARITY 1,
    INDEX idx_network (network) TYPE bloom_filter GRANULARITY 1,
    INDEX idx_tx_hash (tx_hash) TYPE bloom_filter GRANULARITY 1
) ENGINE = ReplacingMergeTree
ORDER BY (network, block_number, tx_hash, log_index);

CREATE TABLE IF NOT EXISTS poly_dearboard_conditional_tokens.condition_resolution (
    contract_address    FixedString(42),
    condition_id        String,
    oracle              FixedString(42),
    question_id         String,
    outcome_slot_count  String,
    payout_numerators   Array(String),
    tx_hash             FixedString(66),
    block_number        UInt64,
    block_timestamp     Nullable(DateTime('UTC')),
    block_hash          FixedString(66),
    network             String,
    tx_index            UInt64,
    log_index           UInt64,

    INDEX idx_block_num (block_number) TYPE minmax GRANULARITY 1,
    INDEX idx_timestamp (block_timestamp) TYPE bloom_filter GRANULARITY 1,
    INDEX idx_condition_id (condition_id) TYPE bloom_filter GRANULARITY 1,
    INDEX idx_tx_hash (tx_hash) TYPE bloom_filter GRANULARITY 1
) ENGINE = ReplacingMergeTree
ORDER BY (network, block_number, tx_hash, log_index);

-- =============================================================================
-- 2. Normalized trades target table
-- =============================================================================

CREATE DATABASE IF NOT EXISTS poly_dearboard;

CREATE TABLE IF NOT EXISTS poly_dearboard.trades (
    exchange          String,
    trader            FixedString(42),
    side              String,
    asset_id          String,
    amount            Decimal128(6),
    price             Decimal128(10),
    usdc_amount       Decimal128(6),
    fee               Decimal128(6),
    order_hash        String,
    tx_hash           FixedString(66),
    block_number      UInt64,
    block_timestamp   Nullable(DateTime('UTC')),
    log_index         UInt64,
    network           String
) ENGINE = ReplacingMergeTree
ORDER BY (trader, block_number, tx_hash, log_index, side);

-- =============================================================================
-- 3. Materialized views: OrderFilled → normalized trades
--
--    CTF/NegRisk exchanges emit N+1 OrderFilled per match:
--      - N "maker fills": (maker=real_maker, taker=real_taker, ...)
--      - 1 "taker summary": (maker=real_taker, taker=exchange_contract, ...)
--
--    We ONLY record the `maker` field as the trader, because:
--      - In maker fills, `maker` = the real maker
--      - In taker summary, `maker` = takerOrder.maker = the real taker
--      - The exchange contract only appears in the `taker` field, never `maker`
--
--    This avoids double-counting, incorrect side attribution in MINT scenarios,
--    and volume inflation. See: Paradigm "Polymarket Volume Is Being Double-Counted"
--
--    Fee note: per-fill fees are charged to the taker but recorded in maker fill
--    events. The taker summary has fee=0. We set fee=0 for all rows to avoid
--    misattribution; fee tracking can be added separately later.
-- =============================================================================

-- ── CTF Exchange ─────────────────────────────────────────────────────────────

-- maker_asset_id == '0' → maker provides USDC → BUY
CREATE MATERIALIZED VIEW IF NOT EXISTS poly_dearboard.mv_ctf_buy
TO poly_dearboard.trades
AS SELECT
    'ctf' AS exchange,
    maker AS trader,
    'buy' AS side,
    taker_asset_id AS asset_id,
    toDecimal128(taker_amount_filled, 6) / 1000000 AS amount,
    round(toDecimal128(maker_amount_filled, 6) / toDecimal128(taker_amount_filled, 6), 10) AS price,
    toDecimal128(maker_amount_filled, 6) / 1000000 AS usdc_amount,
    toDecimal128('0', 6) AS fee,
    order_hash, tx_hash, block_number, block_timestamp, log_index, network
FROM poly_dearboard_ctf_exchange.order_filled
WHERE maker_asset_id = '0';

-- taker_asset_id == '0' → maker provides tokens → SELL
CREATE MATERIALIZED VIEW IF NOT EXISTS poly_dearboard.mv_ctf_sell
TO poly_dearboard.trades
AS SELECT
    'ctf' AS exchange,
    maker AS trader,
    'sell' AS side,
    maker_asset_id AS asset_id,
    toDecimal128(maker_amount_filled, 6) / 1000000 AS amount,
    round(toDecimal128(taker_amount_filled, 6) / toDecimal128(maker_amount_filled, 6), 10) AS price,
    toDecimal128(taker_amount_filled, 6) / 1000000 AS usdc_amount,
    toDecimal128('0', 6) AS fee,
    order_hash, tx_hash, block_number, block_timestamp, log_index, network
FROM poly_dearboard_ctf_exchange.order_filled
WHERE taker_asset_id = '0';

-- ── NegRisk Exchange ─────────────────────────────────────────────────────────

-- maker_asset_id == '0' → maker provides USDC → BUY
CREATE MATERIALIZED VIEW IF NOT EXISTS poly_dearboard.mv_neg_risk_buy
TO poly_dearboard.trades
AS SELECT
    'neg_risk' AS exchange,
    maker AS trader,
    'buy' AS side,
    taker_asset_id AS asset_id,
    toDecimal128(taker_amount_filled, 6) / 1000000 AS amount,
    round(toDecimal128(maker_amount_filled, 6) / toDecimal128(taker_amount_filled, 6), 10) AS price,
    toDecimal128(maker_amount_filled, 6) / 1000000 AS usdc_amount,
    toDecimal128('0', 6) AS fee,
    order_hash, tx_hash, block_number, block_timestamp, log_index, network
FROM poly_dearboard_neg_risk_ctf_exchange.order_filled
WHERE maker_asset_id = '0';

-- taker_asset_id == '0' → maker provides tokens → SELL
CREATE MATERIALIZED VIEW IF NOT EXISTS poly_dearboard.mv_neg_risk_sell
TO poly_dearboard.trades
AS SELECT
    'neg_risk' AS exchange,
    maker AS trader,
    'sell' AS side,
    maker_asset_id AS asset_id,
    toDecimal128(maker_amount_filled, 6) / 1000000 AS amount,
    round(toDecimal128(taker_amount_filled, 6) / toDecimal128(maker_amount_filled, 6), 10) AS price,
    toDecimal128(taker_amount_filled, 6) / 1000000 AS usdc_amount,
    toDecimal128('0', 6) AS fee,
    order_hash, tx_hash, block_number, block_timestamp, log_index, network
FROM poly_dearboard_neg_risk_ctf_exchange.order_filled
WHERE taker_asset_id = '0';

-- =============================================================================
-- 4. Resolved prices: on-chain ConditionResolution → exact price per asset
-- =============================================================================

CREATE TABLE IF NOT EXISTS poly_dearboard.resolved_prices (
    asset_id       String,
    resolved_price String,
    condition_id   String,
    block_number   UInt64
) ENGINE = ReplacingMergeTree
ORDER BY (asset_id);
