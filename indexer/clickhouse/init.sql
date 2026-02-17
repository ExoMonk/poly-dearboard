-- Poly-Dearboard ClickHouse Schema
-- Pre-creates rindexer raw event tables + normalized trades table + materialized views.
-- rindexer's CREATE TABLE IF NOT EXISTS will safely skip tables that already exist.

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
ORDER BY (network, block_number, tx_hash, log_index);

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
ORDER BY (network, block_number, tx_hash, log_index);

-- Conditional Tokens
CREATE DATABASE IF NOT EXISTS poly_dearboard_conditional_tokens;

CREATE TABLE IF NOT EXISTS poly_dearboard_conditional_tokens.payout_redemption (
    contract_address  FixedString(42),
    redeemer          FixedString(42),
    collection_id     String,
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
-- =============================================================================

-- CTF Exchange: maker_asset_id == '0' → maker buys, taker sells
CREATE MATERIALIZED VIEW IF NOT EXISTS poly_dearboard.mv_ctf_trades
TO poly_dearboard.trades
AS
-- Maker buys (provides USDC)
SELECT
    'ctf' AS exchange,
    maker AS trader,
    'buy' AS side,
    taker_asset_id AS asset_id,
    toDecimal128(taker_amount_filled, 0) / 1000000 AS amount,
    round(toDecimal128(maker_amount_filled, 0) / toDecimal128(taker_amount_filled, 0), 10) AS price,
    toDecimal128(maker_amount_filled, 0) / 1000000 AS usdc_amount,
    toDecimal128(0, 6) AS fee,
    order_hash,
    tx_hash,
    block_number,
    block_timestamp,
    log_index,
    network
FROM poly_dearboard_ctf_exchange.order_filled
WHERE maker_asset_id = '0'

UNION ALL

-- Taker sells (receives USDC)
SELECT
    'ctf' AS exchange,
    taker AS trader,
    'sell' AS side,
    taker_asset_id AS asset_id,
    toDecimal128(taker_amount_filled, 0) / 1000000 AS amount,
    round(toDecimal128(maker_amount_filled, 0) / toDecimal128(taker_amount_filled, 0), 10) AS price,
    toDecimal128(maker_amount_filled, 0) / 1000000 AS usdc_amount,
    toDecimal128(fee, 0) / 1000000 AS fee,
    order_hash,
    tx_hash,
    block_number,
    block_timestamp,
    log_index,
    network
FROM poly_dearboard_ctf_exchange.order_filled
WHERE maker_asset_id = '0'

UNION ALL

-- Maker sells (taker provides USDC)
SELECT
    'ctf' AS exchange,
    maker AS trader,
    'sell' AS side,
    maker_asset_id AS asset_id,
    toDecimal128(maker_amount_filled, 0) / 1000000 AS amount,
    round(toDecimal128(taker_amount_filled, 0) / toDecimal128(maker_amount_filled, 0), 10) AS price,
    toDecimal128(taker_amount_filled, 0) / 1000000 AS usdc_amount,
    toDecimal128(0, 6) AS fee,
    order_hash,
    tx_hash,
    block_number,
    block_timestamp,
    log_index,
    network
FROM poly_dearboard_ctf_exchange.order_filled
WHERE taker_asset_id = '0'

UNION ALL

-- Taker buys (provides USDC)
SELECT
    'ctf' AS exchange,
    taker AS trader,
    'buy' AS side,
    maker_asset_id AS asset_id,
    toDecimal128(maker_amount_filled, 0) / 1000000 AS amount,
    round(toDecimal128(taker_amount_filled, 0) / toDecimal128(maker_amount_filled, 0), 10) AS price,
    toDecimal128(taker_amount_filled, 0) / 1000000 AS usdc_amount,
    toDecimal128(fee, 0) / 1000000 AS fee,
    order_hash,
    tx_hash,
    block_number,
    block_timestamp,
    log_index,
    network
FROM poly_dearboard_ctf_exchange.order_filled
WHERE taker_asset_id = '0';

-- NegRisk Exchange: identical logic, different source table and exchange tag
CREATE MATERIALIZED VIEW IF NOT EXISTS poly_dearboard.mv_neg_risk_trades
TO poly_dearboard.trades
AS
SELECT
    'neg_risk' AS exchange,
    maker AS trader,
    'buy' AS side,
    taker_asset_id AS asset_id,
    toDecimal128(taker_amount_filled, 0) / 1000000 AS amount,
    round(toDecimal128(maker_amount_filled, 0) / toDecimal128(taker_amount_filled, 0), 10) AS price,
    toDecimal128(maker_amount_filled, 0) / 1000000 AS usdc_amount,
    toDecimal128(0, 6) AS fee,
    order_hash,
    tx_hash,
    block_number,
    block_timestamp,
    log_index,
    network
FROM poly_dearboard_neg_risk_ctf_exchange.order_filled
WHERE maker_asset_id = '0'

UNION ALL

SELECT
    'neg_risk' AS exchange,
    taker AS trader,
    'sell' AS side,
    taker_asset_id AS asset_id,
    toDecimal128(taker_amount_filled, 0) / 1000000 AS amount,
    round(toDecimal128(maker_amount_filled, 0) / toDecimal128(taker_amount_filled, 0), 10) AS price,
    toDecimal128(maker_amount_filled, 0) / 1000000 AS usdc_amount,
    toDecimal128(fee, 0) / 1000000 AS fee,
    order_hash,
    tx_hash,
    block_number,
    block_timestamp,
    log_index,
    network
FROM poly_dearboard_neg_risk_ctf_exchange.order_filled
WHERE maker_asset_id = '0'

UNION ALL

SELECT
    'neg_risk' AS exchange,
    maker AS trader,
    'sell' AS side,
    maker_asset_id AS asset_id,
    toDecimal128(maker_amount_filled, 0) / 1000000 AS amount,
    round(toDecimal128(taker_amount_filled, 0) / toDecimal128(maker_amount_filled, 0), 10) AS price,
    toDecimal128(taker_amount_filled, 0) / 1000000 AS usdc_amount,
    toDecimal128(0, 6) AS fee,
    order_hash,
    tx_hash,
    block_number,
    block_timestamp,
    log_index,
    network
FROM poly_dearboard_neg_risk_ctf_exchange.order_filled
WHERE taker_asset_id = '0'

UNION ALL

SELECT
    'neg_risk' AS exchange,
    taker AS trader,
    'buy' AS side,
    maker_asset_id AS asset_id,
    toDecimal128(maker_amount_filled, 0) / 1000000 AS amount,
    round(toDecimal128(taker_amount_filled, 0) / toDecimal128(maker_amount_filled, 0), 10) AS price,
    toDecimal128(taker_amount_filled, 0) / 1000000 AS usdc_amount,
    toDecimal128(fee, 0) / 1000000 AS fee,
    order_hash,
    tx_hash,
    block_number,
    block_timestamp,
    log_index,
    network
FROM poly_dearboard_neg_risk_ctf_exchange.order_filled
WHERE taker_asset_id = '0';
