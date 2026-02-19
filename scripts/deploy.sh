#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
SSH_KEY="${SSH_KEY:?Usage: SSH_KEY=~/.ssh/key.pem VPS_HOST=<ip-or-hostname> make deploy}"
VPS_HOST="${VPS_HOST:?Usage: SSH_KEY=~/.ssh/key.pem VPS_HOST=<ip-or-hostname> make deploy}"
VPS_USER="${VPS_USER:-ubuntu}"
REMOTE_DIR="/opt/poly-dearboard"

SSH_CMD="ssh -i $SSH_KEY -o StrictHostKeyChecking=no $VPS_USER@$VPS_HOST"
SCP_CMD="scp -i $SSH_KEY -o StrictHostKeyChecking=no"

echo "Syncing config files to $VPS_HOST..."

# Ensure remote directory structure exists (needs sudo for /opt, then chown to user)
$SSH_CMD "sudo mkdir -p $REMOTE_DIR/deployments/polyderboard-prod $REMOTE_DIR/indexer/clickhouse $REMOTE_DIR/indexer/abi && sudo chown -R $VPS_USER:$VPS_USER $REMOTE_DIR"

# Sync compose + config files
$SCP_CMD "$ROOT/deployments/polyderboard-prod/docker-compose.prod.yml" \
         "$ROOT/deployments/polyderboard-prod/Caddyfile" \
         "$ROOT/deployments/polyderboard-prod/.env.prod" \
         "$VPS_USER@$VPS_HOST:$REMOTE_DIR/deployments/polyderboard-prod/"

$SCP_CMD "$ROOT/indexer/erpc_conf.yaml" \
         "$ROOT/indexer/polywatcher.yaml" \
         "$VPS_USER@$VPS_HOST:$REMOTE_DIR/indexer/"

$SCP_CMD "$ROOT/indexer/abi/"* \
         "$VPS_USER@$VPS_HOST:$REMOTE_DIR/indexer/abi/"

echo "Pulling latest images and restarting..."
$SSH_CMD "cd $REMOTE_DIR/deployments/polyderboard-prod && \
    docker compose -f docker-compose.prod.yml pull && \
    docker compose -f docker-compose.prod.yml up -d && \
    docker compose -f docker-compose.prod.yml restart erpc indexer"

echo ""
echo "Waiting for health check..."
sleep 5
$SSH_CMD "curl -sf http://localhost:3001/api/health" && echo " API healthy" || echo " API not responding yet (may need more time)"

echo ""
echo "Deploy complete. Services on $VPS_HOST:"
$SSH_CMD "docker ps --format 'table {{.Names}}\t{{.Status}}'"
