import { useState, useRef, useEffect, useMemo } from "react";
import { useParams, Link } from "react-router-dom";
import { useQuery } from "@tanstack/react-query";
import { motion } from "motion/react";
import { fetchRecentTrades, fetchMarketResolve, fetchMarketRedemptions } from "../api";
import Spinner from "../components/Spinner";
import LivePriceChart from "../charts/LivePriceChart";
import useMarketWs from "../hooks/useMarketWs";
import useTradeWs from "../hooks/useTradeWs";
import {
  formatUsd,
  formatNumber,
  timeAgo,
  shortenAddress,
  polygonscanTx,
} from "../lib/format";
import { staggerContainer, statCardVariants } from "../lib/motion";
import {
  freshnessLabel,
} from "../lib/streamControls";

const MAX_RENDER_TRADES = 200;

function formatCents(priceStr: string): string {
  const num = parseFloat(priceStr);
  if (isNaN(num)) return "\u2014";
  return `${(num * 100).toFixed(2)}\u00a2`;
}

export default function MarketDetail() {
  const { tokenId } = useParams<{ tokenId: string }>();
  const decodedTokenIds = tokenId ? decodeURIComponent(tokenId) : "";
  const tokenList = useMemo(
    () => decodedTokenIds.split(",").map((s) => s.trim()),
    [decodedTokenIds],
  );

  const prevIdsRef = useRef<Set<string>>(new Set());
  const [newIds, setNewIds] = useState<Set<string>>(new Set());
  const [nowTs, setNowTs] = useState(() => Date.now());

  const { data, isLoading, error } = useQuery({
    queryKey: ["marketTrades", decodedTokenIds],
    queryFn: () => fetchRecentTrades({ limit: 50, token_id: decodedTokenIds }),
    enabled: !!decodedTokenIds,
  });

  const { data: resolved } = useQuery({
    queryKey: ["marketResolve", decodedTokenIds],
    queryFn: () => fetchMarketResolve(decodedTokenIds),
    enabled: !!decodedTokenIds,
    staleTime: Infinity,
  });

  // Live trade stream from our backend WS
  const { liveTrades, connected: tradeWsConnected, lastEventAt } = useTradeWs({
    tokenIds: decodedTokenIds,
  });
  const feedFreshness = tradeWsConnected ? "Live" : freshnessLabel(lastEventAt, nowTs);

  useEffect(() => {
    const timer = window.setInterval(() => setNowTs(Date.now()), 1000);
    return () => window.clearInterval(timer);
  }, []);

  useEffect(() => {
    setNewIds(new Set());
  }, [decodedTokenIds]);

  // Merge HTTP backfill + WS live trades (dedup by tx_hash)
  const mergedTrades = useMemo(() => {
    const httpTrades = data?.trades ?? [];
    if (!liveTrades.length) return httpTrades;
    const seen = new Set(httpTrades.map((t) => t.tx_hash));
    const newFromWs = liveTrades.filter((t) => !seen.has(t.tx_hash));
    return [...newFromWs, ...httpTrades];
  }, [data?.trades, liveTrades]);

  const tableTrades = useMemo(() => {
    const httpTrades = data?.trades ?? [];
    if (!liveTrades.length) return httpTrades;
    const seen = new Set(httpTrades.map((t) => t.tx_hash));
    const newFromWs = liveTrades.filter((t) => !seen.has(t.tx_hash));
    return [...newFromWs, ...httpTrades].slice(0, MAX_RENDER_TRADES);
  }, [data?.trades, liveTrades]);

  // Highlight newly appeared trades
  useEffect(() => {
    if (!tableTrades.length) return;
    const currentIds = new Set(tableTrades.map((t) => t.tx_hash));
    const fresh = new Set<string>();
    for (const id of currentIds) {
      if (!prevIdsRef.current.has(id)) fresh.add(id);
    }
    if (fresh.size > 0 && prevIdsRef.current.size > 0) {
      setNewIds(fresh);
      const timer = setTimeout(() => setNewIds(new Set()), 2000);
      prevIdsRef.current = currentIds;
      return () => clearTimeout(timer);
    }
    prevIdsRef.current = currentIds;
  }, [tableTrades]);

  // Derive primary/secondary token IDs (stable across renders).
  // For Yes/No markets: yesTokenId=Yes, noTokenId=No.
  // For non-binary markets (e.g. "Finland"/"Canada"): yesTokenId=URL token,
  // noTokenId=complement from Gamma API's all_token_ids.
  const { yesTokenId, noTokenId, outcomeLabels } = useMemo(() => {
    let yes: string | null = null;
    let no: string | null = null;
    let labels: [string, string] = ["Yes", "No"];

    // Try to identify tokens from trade outcomes
    if (data && data.trades.length > 0) {
      const latestByToken = new Map<string, (typeof data.trades)[0]>();
      for (const t of data.trades) {
        if (!latestByToken.has(t.asset_id)) latestByToken.set(t.asset_id, t);
      }
      for (const [id, trade] of latestByToken) {
        const outcome = trade.outcome || resolved?.[id]?.outcome || "";
        if (outcome.toLowerCase() === "yes") yes = id;
        else if (outcome.toLowerCase() === "no") no = id;
      }
    }

    // Standard Yes/No fallback
    if (!yes && !no && tokenList.length >= 2) {
      yes = tokenList[0];
      no = tokenList[1];
    } else if (!yes && no) {
      yes = tokenList.find((id) => id !== no) || null;
      // URL only has the No token — find the Yes complement from resolved data
      if (!yes) {
        const noInfo = resolved?.[no];
        if (noInfo && noInfo.all_token_ids?.length >= 2) {
          yes = noInfo.all_token_ids.find((id) => id !== no) || null;
        }
      }
    } else if (yes && !no) {
      no = tokenList.find((id) => id !== yes) || null;
      // URL only has the Yes token — find the No complement from resolved data
      if (!no) {
        const yesInfo = resolved?.[yes];
        if (yesInfo && yesInfo.all_token_ids?.length >= 2) {
          no = yesInfo.all_token_ids.find((id) => id !== yes) || null;
        }
      }
    }

    // Non-binary market: neither "yes" nor "no" found in outcomes.
    // Use the URL token as primary and find its complement from resolved data.
    if (!yes && !no && tokenList.length === 1) {
      const urlToken = tokenList[0];
      const info = resolved?.[urlToken];
      if (info && info.all_token_ids?.length >= 2) {
        yes = urlToken;
        no = info.all_token_ids.find((id) => id !== urlToken) || null;
        // Set outcome labels from Gamma API
        const yesIdx = info.all_token_ids.indexOf(urlToken);
        const noIdx = no ? info.all_token_ids.indexOf(no) : -1;
        labels = [
          yesIdx >= 0 ? info.outcomes[yesIdx] || "Yes" : "Yes",
          noIdx >= 0 ? info.outcomes[noIdx] || "No" : "No",
        ];
      } else {
        // Single-token, no complement available — still use it as primary
        yes = urlToken;
      }
    }

    // Derive labels for standard Yes/No detection too
    if (yes && no && labels[0] === "Yes" && labels[1] === "No") {
      const yesInfo = resolved?.[yes];
      if (yesInfo && yesInfo.outcomes?.length >= 2) {
        const yesIdx = yesInfo.all_token_ids?.indexOf(yes) ?? -1;
        const noIdx = yesInfo.all_token_ids?.indexOf(no) ?? -1;
        const yLabel = yesIdx >= 0 ? yesInfo.outcomes[yesIdx] : "";
        const nLabel = noIdx >= 0 ? yesInfo.outcomes[noIdx] : "";
        if (yLabel && nLabel) labels = [yLabel, nLabel];
      }
    }

    return { yesTokenId: yes, noTokenId: no, outcomeLabels: labels };
  }, [data, resolved, tokenList]);

  // Polymarket WSS for live price stream + our trades merged into one timeline
  const { priceHistory, tradeMarkers, bidAsk, status, latestYesPrice } = useMarketWs({
    yesTokenId,
    noTokenId,
    trades: mergedTrades,
  });

  // Derive prices for PriceBar (WSS takes priority over trade data)
  const fallbackYes = useMemo(() => {
    if (!mergedTrades.length || !yesTokenId) return NaN;
    const anchor = mergedTrades[0];
    const anchorPrice = parseFloat(anchor.price);
    if (isNaN(anchorPrice)) return NaN;
    return anchor.asset_id === yesTokenId ? anchorPrice : 1 - anchorPrice;
  }, [mergedTrades, yesTokenId]);

  const liveYes = latestYesPrice ?? fallbackYes;
  const liveNo = !isNaN(liveYes) ? 1 - liveYes : NaN;

  // Find condition_id and resolved status from any resolved entry
  const conditionId = useMemo(() => {
    if (!resolved) return undefined;
    for (const info of Object.values(resolved)) {
      if (info.condition_id) return info.condition_id;
    }
    return undefined;
  }, [resolved]);

  const firstTrade = mergedTrades[0];
  const resolvedFirst = firstTrade && resolved?.[firstTrade.asset_id];
  const isResolved = resolvedFirst ? !resolvedFirst.active : false;

  const { data: redemptionsData } = useQuery({
    queryKey: ["market-redemptions", conditionId],
    queryFn: () => fetchMarketRedemptions(conditionId!),
    enabled: !!conditionId && isResolved,
  });

  if (isLoading) return <Spinner />;
  if (error)
    return (
      <div className="text-[var(--neon-red)] text-center py-10">
        Failed to load market data
      </div>
    );
  const question =
    resolvedFirst?.question || firstTrade?.question || "Unknown Market";

  return (
    <div className="space-y-8">
      {/* Header */}
      <div>
        <motion.div whileHover={{ x: -4 }} className="inline-block">
          <Link
            to="/activity"
            className="text-sm text-[var(--text-secondary)] hover:text-[var(--accent-blue)] transition-colors duration-200"
          >
            &larr; Back to Activity
          </Link>
        </motion.div>
        <h1 className="text-2xl font-black gradient-text tracking-tight mt-3 glitch-text">
          {question}
        </h1>
      </div>

      {/* Stats + Yes/No Prices */}
      {mergedTrades.length > 0 && (
        <motion.div
          className="grid grid-cols-3 gap-4"
          variants={staggerContainer}
          initial="initial"
          animate="animate"
        >
          <StatCard label="Trades" value={formatNumber(mergedTrades.length)} />
          <PriceBar yesPrice={liveYes} noPrice={liveNo} labels={outcomeLabels} />
          <StatCard label="Last Trade" value={timeAgo(mergedTrades[0].block_timestamp)} />
        </motion.div>
      )}

      {/* Live Price Chart */}
      {yesTokenId && (
        <LivePriceChart
          priceHistory={priceHistory}
          tradeMarkers={tradeMarkers}
          status={status}
          bidAsk={bidAsk}
        />
      )}

      {/* Live Feed */}
      <div>
        <div className="flex items-center justify-between gap-3 mb-4 flex-wrap">
          <div className="flex items-center gap-3">
          <h2 className="text-lg font-bold gradient-text">Live Feed</h2>
          <span className="flex items-center gap-1.5 text-xs text-[var(--text-secondary)]">
            <span
              className={`w-2 h-2 rounded-full ${
                tradeWsConnected
                  ? "bg-[var(--neon-green)] neon-pulse shadow-[0_0_8px_var(--neon-green)]"
                  : "bg-[var(--neon-red)]"
              }`}
            />
            {tradeWsConnected ? "Live" : "Reconnecting"}
          </span>
            <span
              className={`text-xs px-2 py-1 rounded-full ${
                feedFreshness === "Live" || feedFreshness === "Updated <10s"
                  ? "text-[var(--neon-green)] bg-[var(--neon-green)]/10"
                  : "text-[var(--accent-orange)] bg-[var(--accent-orange)]/10"
              }`}
            >
              {feedFreshness}
            </span>
            </div>
        </div>

        {tableTrades.length > 0 ? (
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
                  {tableTrades.map((t, i) => (
                    <motion.tr
                      key={`${t.tx_hash}-${i}`}
                      initial={{ opacity: 0, x: -8 }}
                      animate={{ opacity: 1, x: 0 }}
                      transition={{ duration: 0.25, delay: i * 0.02 }}
                      className={`border-b border-[var(--border-subtle)] row-glow ${
                        newIds.has(t.tx_hash) ? "delta-flash-neutral" : ""
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
                      <td className="px-4 py-3 text-right font-mono glow-blue text-xs">
                        {formatCents(t.price)}
                      </td>
                      <td className="px-4 py-3 text-right font-mono text-[var(--text-primary)] text-xs">
                        {formatUsd(t.usdc_amount)}
                      </td>
                      <td className="px-4 py-3 hidden md:table-cell">
                        <Link
                          to={`/trader/${t.trader}`}
                          className="text-[var(--accent-blue)]/70 hover:text-[var(--accent-blue)] font-mono text-xs transition-colors duration-200"
                        >
                          {shortenAddress(t.trader)}
                        </Link>
                      </td>
                      <td className="px-4 py-3 text-right hidden lg:table-cell">
                        <a
                          href={polygonscanTx(t.tx_hash)}
                          target="_blank"
                          rel="noopener noreferrer"
                          className="text-[var(--text-secondary)] opacity-40 hover:opacity-100 hover:text-[var(--accent-blue)] font-mono text-xs transition-all duration-200"
                        >
                          {shortenAddress(t.tx_hash)}
                        </a>
                      </td>
                    </motion.tr>
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

      {/* Who Cashed Out — only for resolved markets */}
      {isResolved && redemptionsData && redemptionsData.redemptions.length > 0 && (
        <div>
          <div className="flex items-center gap-3 mb-4">
            <h2 className="text-lg font-bold gradient-text">Who Cashed Out</h2>
            <span className="text-xs text-[var(--text-secondary)]">
              {redemptionsData.total} redemption{redemptionsData.total !== 1 ? "s" : ""} &middot;{" "}
              {formatUsd(String(redemptionsData.redemptions.reduce((sum, r) => sum + r.payout_usdc, 0)))} total
            </span>
          </div>
          <div className="glass overflow-hidden">
            <div className="overflow-x-auto">
              <table className="w-full text-sm">
                <thead>
                  <tr className="border-b border-[var(--border-glow)] text-[var(--text-secondary)] text-xs uppercase tracking-widest">
                    <th className="px-4 py-3 text-left">Trader</th>
                    <th className="px-4 py-3 text-right">Payout</th>
                    <th className="px-4 py-3 text-right">Date</th>
                    <th className="px-4 py-3 text-right">Tx</th>
                  </tr>
                </thead>
                <tbody>
                  {redemptionsData.redemptions.map((r, i) => (
                    <motion.tr
                      key={`${r.tx_hash}-${i}`}
                      initial={{ opacity: 0, x: -8 }}
                      animate={{ opacity: 1, x: 0 }}
                      transition={{ duration: 0.25, delay: i * 0.02 }}
                      className="border-b border-[var(--border-subtle)] row-glow"
                    >
                      <td className="px-4 py-3">
                        <Link
                          to={`/trader/${r.redeemer}`}
                          className="text-[var(--accent-cyan)] hover:text-[var(--accent-blue)] font-mono text-xs transition-colors duration-200"
                        >
                          {shortenAddress(r.redeemer)}
                        </Link>
                      </td>
                      <td className="px-4 py-3 text-right font-mono glow-green">
                        {formatUsd(String(r.payout_usdc))}
                      </td>
                      <td className="px-4 py-3 text-right text-[var(--text-secondary)] text-xs">
                        {r.redeemed_at ? timeAgo(r.redeemed_at) : "—"}
                      </td>
                      <td className="px-4 py-3 text-right">
                        <a
                          href={polygonscanTx(r.tx_hash)}
                          target="_blank"
                          rel="noopener noreferrer"
                          className="text-[var(--text-secondary)] opacity-40 hover:opacity-100 hover:text-[var(--accent-blue)] font-mono text-xs transition-all duration-200"
                        >
                          {shortenAddress(r.tx_hash)}
                        </a>
                      </td>
                    </motion.tr>
                  ))}
                </tbody>
              </table>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}

function StatCard({ label, value }: { label: string; value: string }) {
  return (
    <motion.div variants={statCardVariants} className="glass p-4 gradient-border-top">
      <div className="text-xs text-[var(--text-secondary)] mb-2 uppercase tracking-wider">{label}</div>
      <div className="text-xl font-bold font-mono text-[var(--text-primary)]">{value}</div>
    </motion.div>
  );
}

function PriceBar({ yesPrice, noPrice, labels = ["Yes", "No"] }: { yesPrice: number; noPrice: number; labels?: [string, string] }) {
  const yesPct = isNaN(yesPrice) ? 50 : Math.round(yesPrice * 100);
  const noPct = isNaN(noPrice) ? 50 : Math.round(noPrice * 100);

  return (
    <motion.div variants={statCardVariants} className="glass p-4 gradient-border-top">
      <div className="text-xs text-[var(--text-secondary)] mb-3 uppercase tracking-wider">
        Prices
      </div>
      <div className="flex items-center justify-between mb-2">
        <span className="text-sm font-bold text-[var(--neon-green)]">
          {labels[0]} {yesPct}&cent;
        </span>
        <span className="text-sm font-bold text-[var(--neon-red)]">
          {labels[1]} {noPct}&cent;
        </span>
      </div>
      <div className="flex h-2 rounded-full overflow-hidden gap-0.5">
        <motion.div
          className="rounded-full bg-[var(--neon-green)]/60"
          initial={{ width: 0 }}
          animate={{ width: `${yesPct}%` }}
          transition={{ type: "spring", stiffness: 120, damping: 20 }}
        />
        <motion.div
          className="rounded-full bg-[var(--neon-red)]/40"
          initial={{ width: 0 }}
          animate={{ width: `${noPct}%` }}
          transition={{ type: "spring", stiffness: 120, damping: 20 }}
        />
      </div>
    </motion.div>
  );
}
