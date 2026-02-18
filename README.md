# Poly Dearboard

On-chain Polymarket leaderboard built from ground-truth trade data. Indexes `OrderFilled` events directly from the CTF Exchange and NegRisk Exchange contracts on Polygon — no reliance on Polymarket's API for trade data.

![Leaderboard](assets/leaderboard.png)

## Features

**Leaderboard** — Rank traders by mark-to-market PnL, volume, or trade count. Scatter chart visualizes efficiency (Return on Volume %) vs performance.

**Trader Detail** — Per-trader stats, open positions valued at live market prices, trade activity chart, and full trade history.

<p align="center">
  <img src="assets/trader_1.png" width="49%" />
  <img src="assets/trader_2.png" width="49%" />
</p>

**Hot Markets** — Most active markets by volume (1h / 24h / 7d) with category tags and trader counts.

![Hot Markets](assets/hot-market.png)

**Market Live Feed** — Per-market real-time trade feed with Yes/No outcome display and price bars.

![Live Feed](assets/live-feed.png)

## Stack

| Layer | Technology |
| --- | --- |
| Indexer | [rindexer](https://github.com/joshstevens19/rindexer) (no-code mode) |
| Storage | ClickHouse (columnar OLAP) |
| Normalization | ClickHouse materialized views |
| API | Rust / Axum |
| Frontend | React + TypeScript + Recharts + Tailwind |
| Chain | Polygon (chain_id: 137) |

## Architecture

```
Polygon (2s blocks)
  ↓ eth_getLogs
rindexer no-code indexer
  ↓ raw events
ClickHouse (OrderFilled, PayoutRedemption tables)
  ↓ materialized views
ClickHouse (normalized trades: trader, side, asset, price, amount)
  ↓ CTE queries
Axum REST API ← React frontend
```

## PnL Methodology

Uses **mark-to-market** PnL rather than pure cash-flow:

```
PnL = sum_per_asset(cash_flow + net_tokens × latest_market_price)
```

This values open positions at the current market price, producing meaningful PnL even with a partial indexing window. Closed positions (net_tokens = 0) reduce to pure cash-flow PnL.

**Known limitation**: Market resolution redemptions (`PayoutRedemption` events) are not included in PnL because the event lacks an `asset_id` field, making per-position matching impossible without deriving token IDs from `keccak256(parentCollectionId, conditionId, indexSet)`. Mark-to-market approximates this well since winning tokens trade near $1.00 before resolution.

## Run

| Command | What it does |
| --- | --- |
| `make indexer` | Starts ClickHouse + rindexer |
| `make serve` | Starts the Axum API on port 3001 |
| `make query` | Runs E2E leaderboard queries |
| `make clean` | Tears down Docker containers + volumes |
| `make frontend` | Runs Polydearboard frontend |

## API

| Endpoint | Description |
| --- | --- |
| `GET /api/leaderboard` | Paginated trader rankings (sort: PnL, volume, trades) |
| `GET /api/trader/{address}` | Single trader aggregate stats |
| `GET /api/trader/{address}/trades` | Trade history with side filter + pagination |
| `GET /api/trader/{address}/positions` | Open positions with market prices |
| `GET /api/markets/hot` | Hot markets by volume (1h/24h/7d) |
| `GET /api/trades/recent` | Live trade feed, filterable by token ID |
| `GET /api/health` | Health check with trade/trader/block counts |

## Data Lake

| Database | Table | Source |
| --- | --- | --- |
| `poly_dearboard_ctf_exchange` | `order_filled` | CTF Exchange OrderFilled events |
| `poly_dearboard_neg_risk_ctf_exchange` | `order_filled` | NegRisk Exchange OrderFilled events |
| `poly_dearboard_conditional_tokens` | `payout_redemption` | ConditionalTokens PayoutRedemption events |
| `poly_dearboard` | `trades` | Normalized trades (via materialized views) |
