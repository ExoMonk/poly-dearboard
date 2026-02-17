#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
COMPOSE="docker compose -f $ROOT/development/docker-compose.yml"

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
