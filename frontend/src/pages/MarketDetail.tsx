import { useState, useRef, useEffect } from "react";
import { useParams, Link } from "react-router-dom";
import { useQuery } from "@tanstack/react-query";
import { fetchRecentTrades, fetchMarketResolve } from "../api";
import Spinner from "../components/Spinner";
import {
  formatUsd,
  formatNumber,
  timeAgo,
  shortenAddress,
  polygonscanTx,
} from "../lib/format";

function formatCents(priceStr: string): string {
  const num = parseFloat(priceStr);
  if (isNaN(num)) return "\u2014";
  return `${(num * 100).toFixed(2)}\u00a2`;
}

export default function MarketDetail() {
  const { tokenId } = useParams<{ tokenId: string }>();
  // token IDs are comma-separated for multi-outcome markets (Yes + No)
  const decodedTokenIds = tokenId ? decodeURIComponent(tokenId) : "";

  const prevIdsRef = useRef<Set<string>>(new Set());
  const [newIds, setNewIds] = useState<Set<string>>(new Set());

  const { data, isLoading, error } = useQuery({
    queryKey: ["marketTrades", decodedTokenIds],
    queryFn: () => fetchRecentTrades({ limit: 50, token_id: decodedTokenIds }),
    enabled: !!decodedTokenIds,
    refetchInterval: 5_000,
  });

  // On-demand resolve: fetch market info for token IDs not in server cache.
  // Fires once per page visit; the server caches the result for future requests.
  const { data: resolved } = useQuery({
    queryKey: ["marketResolve", decodedTokenIds],
    queryFn: () => fetchMarketResolve(decodedTokenIds),
    enabled: !!decodedTokenIds,
    staleTime: Infinity,
  });

  // Highlight newly appeared trades
  useEffect(() => {
    if (!data) return;
    const currentIds = new Set(data.trades.map((t) => t.tx_hash));
    const fresh = new Set<string>();
    for (const id of currentIds) {
      if (!prevIdsRef.current.has(id)) fresh.add(id);
    }
    if (fresh.size > 0 && prevIdsRef.current.size > 0) {
      setNewIds(fresh);
      const timer = setTimeout(() => setNewIds(new Set()), 1500);
      return () => clearTimeout(timer);
    }
    prevIdsRef.current = currentIds;
  }, [data]);

  if (isLoading) return <Spinner />;
  if (error)
    return (
      <div className="text-[var(--neon-red)] text-center py-10">
        Failed to load market data
      </div>
    );

  // Derive question: prefer resolved data (on-demand fetch), fall back to trade data
  const firstTrade = data?.trades[0];
  const resolvedFirst = firstTrade && resolved?.[firstTrade.asset_id];
  const question =
    resolvedFirst?.question || firstTrade?.question || "Unknown Market";

  // Derive Yes/No prices from the most recent trade.
  // Binary outcome tokens always sum to $1, so derive the complement.
  // Token ID order from URL: first = Yes, second = No (Polymarket convention).
  const tokenList = decodedTokenIds.split(",").map((s) => s.trim());
  let yesPrice = NaN;
  let noPrice = NaN;
  if (data && data.trades.length > 0) {
    // Build a map: asset_id → most recent trade (trades are sorted newest-first)
    const latestByToken = new Map<string, (typeof data.trades)[0]>();
    for (const t of data.trades) {
      if (!latestByToken.has(t.asset_id)) latestByToken.set(t.asset_id, t);
    }

    // Identify Yes/No tokens: prefer outcome label (from trade or resolved), fall back to URL order
    let yesTokenId: string | undefined;
    let noTokenId: string | undefined;
    for (const [id, trade] of latestByToken) {
      const outcome = trade.outcome || resolved?.[id]?.outcome || "";
      if (outcome.toLowerCase() === "yes") yesTokenId = id;
      else if (outcome.toLowerCase() === "no") noTokenId = id;
    }
    if (!yesTokenId && !noTokenId && tokenList.length >= 2) {
      yesTokenId = tokenList[0];
      noTokenId = tokenList[1];
    } else if (!yesTokenId && noTokenId) {
      yesTokenId = [...latestByToken.keys()].find((id) => id !== noTokenId);
    } else if (yesTokenId && !noTokenId) {
      noTokenId = [...latestByToken.keys()].find((id) => id !== yesTokenId);
    }

    // Use the most recent trade (index 0) and derive complement
    const anchor = data.trades[0];
    const anchorPrice = parseFloat(anchor.price);
    if (!isNaN(anchorPrice)) {
      if (anchor.asset_id === noTokenId) {
        noPrice = anchorPrice;
        yesPrice = 1 - anchorPrice;
      } else {
        yesPrice = anchorPrice;
        noPrice = 1 - anchorPrice;
      }
    }
  }

  return (
    <div className="space-y-8">
      {/* Header */}
      <div>
        <Link
          to="/activity"
          className="text-sm text-[var(--text-secondary)] hover:text-[var(--accent-cyan)] transition-colors duration-200"
        >
          &larr; Back to Activity
        </Link>
        <h1 className="text-2xl font-black gradient-text tracking-tight mt-3">
          {question}
        </h1>
      </div>

      {/* Stats + Yes/No Prices */}
      {data && data.trades.length > 0 && (
        <div className="grid grid-cols-3 gap-4">
          <StatCard label="Trades" value={formatNumber(data.trades.length)} />
          <PriceBar yesPrice={yesPrice} noPrice={noPrice} />
          <StatCard label="Last Trade" value={timeAgo(data.trades[0].block_timestamp)} />
        </div>
      )}

      {/* Live Feed */}
      <div>
        <div className="flex items-center gap-3 mb-4">
          <h2 className="text-lg font-bold gradient-text">Live Feed</h2>
          <span className="flex items-center gap-1.5 text-xs text-[var(--text-secondary)]">
            <span className="w-2 h-2 rounded-full bg-[var(--neon-green)] animate-pulse shadow-[0_0_8px_var(--neon-green)]" />
            Live
          </span>
        </div>

        {data && data.trades.length > 0 ? (
          <div className="glass overflow-hidden">
            <div className="overflow-x-auto">
              <table className="w-full text-sm">
                <thead>
                  <tr className="border-b border-[var(--border-glow)] text-[var(--text-secondary)] text-xs uppercase tracking-widest">
                    <th className="px-4 py-3 text-left">Time</th>
                    <th className="px-4 py-3 text-center">Side</th>
                    <th className="px-4 py-3 text-center">Outcome</th>
                    <th className="px-4 py-3 text-right">Amount</th>
                    <th className="px-4 py-3 text-right">Price</th>
                    <th className="px-4 py-3 text-right">USDC</th>
                    <th className="px-4 py-3 text-left hidden md:table-cell">Trader</th>
                    <th className="px-4 py-3 text-right hidden lg:table-cell">Tx</th>
                  </tr>
                </thead>
                <tbody>
                  {data.trades.map((t, i) => (
                    <tr
                      key={`${t.tx_hash}-${i}`}
                      className={`border-b border-[var(--border-subtle)] row-glow transition-colors duration-700 ${
                        newIds.has(t.tx_hash) ? "bg-[var(--accent-cyan)]/8" : ""
                      }`}
                    >
                      <td className="px-4 py-3 text-[var(--text-secondary)] whitespace-nowrap text-xs">
                        {timeAgo(t.block_timestamp)}
                      </td>
                      <td className="px-4 py-3 text-center">
                        <span
                          className={`text-xs font-bold px-2.5 py-0.5 rounded-full ${
                            t.side === "buy"
                              ? "bg-[var(--neon-green)]/10 text-[var(--neon-green)] shadow-[0_0_6px_rgba(0,255,136,0.15)]"
                              : "bg-[var(--neon-red)]/10 text-[var(--neon-red)] shadow-[0_0_6px_rgba(255,51,102,0.15)]"
                          }`}
                        >
                          {t.side.toUpperCase()}
                        </span>
                      </td>
                      <td className="px-4 py-3 text-center">
                        {(() => {
                          const outcome = t.outcome || resolved?.[t.asset_id]?.outcome || "";
                          return (
                            <span
                              className={`text-xs font-bold px-2.5 py-0.5 rounded-full ${
                                outcome.toLowerCase() === "yes"
                                  ? "bg-[var(--neon-green)]/10 text-[var(--neon-green)]"
                                  : "bg-[var(--neon-red)]/10 text-[var(--neon-red)]"
                              }`}
                            >
                              {outcome || "\u2014"}
                            </span>
                          );
                        })()}
                      </td>
                      <td className="px-4 py-3 text-right font-mono text-[var(--text-primary)] text-xs">
                        {formatNumber(t.amount)}
                      </td>
                      <td className="px-4 py-3 text-right font-mono glow-cyan text-xs">
                        {formatCents(t.price)}
                      </td>
                      <td className="px-4 py-3 text-right font-mono text-[var(--text-primary)] text-xs">
                        {formatUsd(t.usdc_amount)}
                      </td>
                      <td className="px-4 py-3 hidden md:table-cell">
                        <Link
                          to={`/trader/${t.trader}`}
                          className="text-[var(--accent-cyan)]/70 hover:text-[var(--accent-cyan)] font-mono text-xs transition-colors duration-200"
                        >
                          {shortenAddress(t.trader)}
                        </Link>
                      </td>
                      <td className="px-4 py-3 text-right hidden lg:table-cell">
                        <a
                          href={polygonscanTx(t.tx_hash)}
                          target="_blank"
                          rel="noopener noreferrer"
                          className="text-[var(--text-secondary)] opacity-40 hover:opacity-100 hover:text-[var(--accent-cyan)] font-mono text-xs transition-all duration-200"
                        >
                          {shortenAddress(t.tx_hash)}
                        </a>
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          </div>
        ) : (
          <div className="glass p-8 text-center text-[var(--text-secondary)]">
            No trades found for this market
          </div>
        )}
      </div>
    </div>
  );
}

function StatCard({ label, value }: { label: string; value: string }) {
  return (
    <div className="glass p-4 gradient-border-top">
      <div className="text-xs text-[var(--text-secondary)] mb-2 uppercase tracking-wider">{label}</div>
      <div className="text-xl font-bold font-mono text-[var(--text-primary)]">{value}</div>
    </div>
  );
}

function PriceBar({ yesPrice, noPrice }: { yesPrice: number; noPrice: number }) {
  const yesPct = isNaN(yesPrice) ? 50 : Math.round(yesPrice * 100);
  const noPct = isNaN(noPrice) ? 50 : Math.round(noPrice * 100);

  return (
    <div className="glass p-4 gradient-border-top">
      <div className="text-xs text-[var(--text-secondary)] mb-3 uppercase tracking-wider">
        Prices
      </div>
      <div className="flex items-center justify-between mb-2">
        <span className="text-sm font-bold text-[var(--neon-green)]">
          Yes {yesPct}&cent;
        </span>
        <span className="text-sm font-bold text-[var(--neon-red)]">
          No {noPct}&cent;
        </span>
      </div>
      <div className="flex h-2 rounded-full overflow-hidden gap-0.5">
        <div
          className="rounded-full bg-[var(--neon-green)]/60 transition-all duration-500"
          style={{ width: `${yesPct}%` }}
        />
        <div
          className="rounded-full bg-[var(--neon-red)]/40 transition-all duration-500"
          style={{ width: `${noPct}%` }}
        />
      </div>
    </div>
  );
}
