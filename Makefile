.PHONY: indexer live serve query frontend test-e2e backfill backfill-resolutions backfill-stop prune clean publish deploy

COMPOSE := docker compose -f deployments/polyderboard-dev/docker-compose.yml

indexer: ## Start ClickHouse + rindexer (shows indexer logs)
	@./scripts/indexer.sh

live: ## Same as indexer, but start from current block (no backfill)
	@LIVE=1 ./scripts/indexer.sh

serve: ## Start API server on port 3001
	cargo run

query: ## Run E2E leaderboard queries against the API
	@./scripts/queries.sh

frontend: ## Start frontend dev server on port 5173
	cd frontend && bun run dev

test-e2e: ## Run E2E API tests (requires running server: make serve)
	bun test tests/

backfill: ## Backfill historical blocks: FROM=<block> [TO=<block>] make backfill
	@./scripts/backfill.sh

backfill-stop: ## Stop a running backfill
	@docker stop poly-backfill 2>/dev/null && docker rm poly-backfill 2>/dev/null && echo "Backfill stopped" || echo "No backfill running"

prune: ## Delete data before a block: BEFORE=83125113 make prune
	@test -n "$(BEFORE)" || { echo "Usage: BEFORE=<block_number> make prune"; exit 1; }
	@echo "Deleting all data before block $(BEFORE)..."
	@$(COMPOSE) exec -T clickhouse clickhouse-client --multiquery \
		-q "ALTER TABLE poly_dearboard.trades DELETE WHERE block_number < $(BEFORE); \
		    ALTER TABLE poly_dearboard_ctf_exchange.order_filled DELETE WHERE block_number < $(BEFORE); \
		    ALTER TABLE poly_dearboard_neg_risk_ctf_exchange.order_filled DELETE WHERE block_number < $(BEFORE); \
		    ALTER TABLE poly_dearboard_conditional_tokens.payout_redemption DELETE WHERE block_number < $(BEFORE);"
	@echo "Mutations queued. Data before block $(BEFORE) will be removed."

clean: ## Tear down Docker containers + volumes
	$(COMPOSE) down -v
	@docker rm -f poly-backfill 2>/dev/null || true

publish: ## Build and push API image to GHCR
	@./scripts/publish.sh

deploy: ## Deploy latest image to production VPS
	@./scripts/deploy.sh
