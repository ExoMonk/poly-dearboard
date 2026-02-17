.PHONY: indexer live serve query clean

COMPOSE := docker compose -f development/docker-compose.yml

indexer: ## Start ClickHouse + rindexer (shows indexer logs)
	@./scripts/indexer.sh

live: ## Same as indexer, but start from current block (no backfill)
	@LIVE=1 ./scripts/indexer.sh

serve: ## Start API server on port 3001
	cargo run

query: ## Run E2E leaderboard queries against the API
	@./scripts/queries.sh

clean: ## Tear down Docker containers + volumes
	$(COMPOSE) down -v
