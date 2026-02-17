#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
COMPOSE="docker compose -f $ROOT/development/docker-compose.yml"
YAML="$ROOT/indexer/polywatcher.yaml"

# ── Live mode: patch start_block to current block ───────────────────────────
if [ "${LIVE:-}" = "1" ]; then
    echo "Fetching current Polygon block..."
    BLOCK=$(curl -s -X POST https://polygon-rpc.com \
        -H "Content-Type: application/json" \
        -d '{"jsonrpc":"2.0","method":"eth_blockNumber","params":[],"id":1}' \
        | python3 -c "import sys,json; print(int(json.load(sys.stdin)['result'],16))")
    echo "Patching start_block → $BLOCK (live mode, no backfill)"
    sed -i.bak "s/start_block: \"[0-9]*\"/start_block: \"$BLOCK\"/" "$YAML"
    trap 'mv "$YAML.bak" "$YAML"; echo "Restored original start_block"' EXIT
fi

# ── Start ClickHouse + eRPC ─────────────────────────────────────────────────
echo "Starting ClickHouse + eRPC..."
$COMPOSE up -d clickhouse erpc

echo -n "Waiting for ClickHouse"
until $COMPOSE exec -T clickhouse clickhouse-client --query "SELECT 1" >/dev/null 2>&1; do
    echo -n "."
    sleep 1
done
echo " ready"

echo -n "Waiting for eRPC"
until curl -s http://localhost:4000 >/dev/null 2>&1; do
    echo -n "."
    sleep 1
done
echo " ready"

# ── Print endpoints ──────────────────────────────────────────────────────────
echo ""
echo "┌─────────────────────────────────────────────────────────────┐"
echo "│  Poly-Dearboard Indexer                                     │"
echo "├─────────────────────────────────────────────────────────────┤"
echo "│  ClickHouse Play UI    http://localhost:8123/play           │"
echo "│  ClickHouse Prometheus http://localhost:9363/metrics        │"
echo "│  eRPC Proxy            http://localhost:4000                │"
echo "│  eRPC Admin            http://localhost:4001                │"
echo "│  rindexer Health       http://localhost:8080/health         │"
echo "│  rindexer Metrics      http://localhost:8080/metrics        │"
echo "├─────────────────────────────────────────────────────────────┤"
echo "│  API (run separately)  make serve → http://localhost:3001   │"
echo "│  E2E queries           make query                           │"
echo "└─────────────────────────────────────────────────────────────┘"
echo ""

# ── Start rindexer (foreground — shows logs) ─────────────────────────────────
echo "Starting rindexer indexer..."
$COMPOSE up indexer
