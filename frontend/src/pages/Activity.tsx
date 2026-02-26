import { useState, useEffect, useMemo, useRef } from "react";
import { useQuery } from "@tanstack/react-query";
import { useNavigate } from "react-router-dom";
import { motion } from "motion/react";
import { fetchHotMarkets } from "../api";
import Spinner from "../components/Spinner";
import { formatUsd, formatNumber, timeAgo } from "../lib/format";
import { tapScale } from "../lib/motion";
import {
  loadStreamControls,
  saveStreamControls,
  freshnessLabel,
  type StreamUiControls,
} from "../lib/streamControls";

const STREAM_CONTROLS_KEY = "pd_stream_controls_activity";

const PERIODS = [
  { label: "1H", value: "1h" },
  { label: "24H", value: "24h" },
  { label: "7D", value: "7d" },
] as const;

export default function Activity() {
  const [period, setPeriod] = useState("24h");
  const [controls, setControls] = useState<StreamUiControls>(() => loadStreamControls(STREAM_CONTROLS_KEY));

  useEffect(() => {
    saveStreamControls(STREAM_CONTROLS_KEY, controls);
  }, [controls]);

  return (
    <div className="space-y-8">
      <div className="flex items-center justify-between">
        <h1 className="text-3xl font-black gradient-text tracking-tight glitch-text">Activity</h1>
        <button
          type="button"
          aria-pressed={controls.compact}
          onClick={() => setControls((prev) => ({ ...prev, compact: !prev.compact }))}
          className={`px-3 py-1.5 text-xs rounded-full font-semibold border transition-colors ${
            controls.compact
              ? "bg-[var(--accent-blue)]/12 text-[var(--accent-blue)] border-[var(--accent-blue)]/30"
              : "text-[var(--text-secondary)] border-[var(--border-glow)] hover:text-[var(--text-primary)]"
          }`}
        >
          Compact
        </button>
      </div>
      <HotMarkets period={period} setPeriod={setPeriod} compact={controls.compact} />
    </div>
  );
}

function HotMarkets({ period, setPeriod, compact }: { period: string; setPeriod: (p: string) => void; compact: boolean }) {
  const navigate = useNavigate();
  const [deltaByKey, setDeltaByKey] = useState<Record<string, "positive" | "negative" | "neutral">>({});
  const [lastUpdatedAt, setLastUpdatedAt] = useState<number | null>(null);
  const [nowTs, setNowTs] = useState(() => Date.now());
  const prevSnapshotRef = useRef<Record<string, { rank: number; volume: number; trades: number; traders: number }>>({});

  const { data, isLoading, error } = useQuery({
    queryKey: ["hotMarkets", period],
    queryFn: () => fetchHotMarkets({ period, limit: 20 }),
    refetchInterval: 10_000,
  });

  useEffect(() => {
    const timer = window.setInterval(() => setNowTs(Date.now()), 1000);
    return () => window.clearInterval(timer);
  }, []);

  useEffect(() => {
    if (!data) return;
    setLastUpdatedAt(Date.now());

    const prev = prevSnapshotRef.current;
    const next: Record<string, { rank: number; volume: number; trades: number; traders: number }> = {};
    const changes: Record<string, "positive" | "negative" | "neutral"> = {};

    data.markets.forEach((market, index) => {
      const tokenKey = market.token_id;
      const current = {
        rank: index + 1,
        volume: Number.parseFloat(market.volume),
        trades: market.trade_count,
        traders: market.unique_traders,
      };
      next[tokenKey] = current;

      const previous = prev[tokenKey];
      if (!previous) {
        if (Object.keys(prev).length > 0) {
          changes[`${tokenKey}:rank`] = "neutral";
        }
        return;
      }

      if (current.rank !== previous.rank) {
        changes[`${tokenKey}:rank`] = current.rank < previous.rank ? "positive" : "negative";
      }
      if (current.volume !== previous.volume) {
        changes[`${tokenKey}:volume`] = current.volume > previous.volume ? "positive" : "negative";
      }
      if (current.trades !== previous.trades) {
        changes[`${tokenKey}:trades`] = current.trades > previous.trades ? "positive" : "negative";
      }
      if (current.traders !== previous.traders) {
        changes[`${tokenKey}:unique`] = current.traders > previous.traders ? "positive" : "negative";
      }
    });

    prevSnapshotRef.current = next;
    if (Object.keys(changes).length > 0) {
      setDeltaByKey(changes);
      const timer = window.setTimeout(() => setDeltaByKey({}), 2000);
      return () => window.clearTimeout(timer);
    }
  }, [data]);

  const freshness = useMemo(() => freshnessLabel(lastUpdatedAt, nowTs), [lastUpdatedAt, nowTs]);
  const freshnessDisplay = freshness === "Stale" ? "Refreshing..." : freshness;

  const deltaClass = (key: string): string => {
    const kind = deltaByKey[key];
    if (kind === "positive") return "delta-flash-positive";
    if (kind === "negative") return "delta-flash-negative";
    if (kind === "neutral") return "delta-flash-neutral";
    return "";
  };

  const maxVolume = data?.markets.reduce((max, m) => Math.max(max, parseFloat(m.volume)), 0) ?? 1;

  return (
    <div>
      <div className="flex items-center justify-between mb-4">
        <h2 className="text-lg font-bold gradient-text">Hot Markets</h2>
        <div className="flex items-center gap-2 flex-wrap justify-end">
          <span
            className={`text-xs px-2 py-1 rounded-full ${
              freshness === "Updated <10s"
                ? "text-[var(--neon-green)] bg-[var(--neon-green)]/10"
                : "text-[var(--accent-orange)] bg-[var(--accent-orange)]/10"
            }`}
          >
            {freshnessDisplay}
          </span>
          <div className="flex gap-1">
            {PERIODS.map((p) => (
              <motion.button
                key={p.value}
                onClick={() => setPeriod(p.value)}
                whileTap={tapScale}
                className={`px-4 py-1.5 text-xs rounded-full font-medium transition-all duration-200 ${
                  period === p.value
                    ? "bg-[var(--accent-blue)]/10 text-[var(--accent-blue)] border border-[var(--accent-blue)]/30 shadow-[0_0_8px_rgba(59,130,246,0.15)]"
                    : "text-[var(--text-secondary)] border border-transparent hover:text-[var(--text-primary)] hover:border-[var(--border-glow)]"
                }`}
              >
                {p.label}
              </motion.button>
            ))}
          </div>
        </div>
      </div>

      {isLoading ? (
        <Spinner />
      ) : error ? (
        <div className="text-[var(--neon-red)] text-center py-10">Failed to load hot markets</div>
      ) : data && data.markets.length > 0 ? (
        <div className="glass overflow-hidden">
          <div className="overflow-x-auto">
            <table className={`w-full ${compact ? "text-xs" : "text-sm"}`}>
              <thead>
                <tr className="border-b border-[var(--border-glow)] text-[var(--text-secondary)] text-xs uppercase tracking-widest">
                  <th className="px-4 py-3 text-left w-10">#</th>
                  <th className="px-4 py-3 text-left">Market</th>
                  <th className="px-4 py-3 text-right">Volume</th>
                  <th className="px-4 py-3 text-right">Trades</th>
                  <th className="px-4 py-3 text-right hidden md:table-cell">Traders</th>
                  <th className="px-4 py-3 text-left hidden lg:table-cell">Category</th>
                  <th className="px-4 py-3 text-right hidden lg:table-cell">Last Trade</th>
                </tr>
              </thead>
              <tbody>
                {data.markets.map((m, i) => {
                  const vol = parseFloat(m.volume);
                  const pct = maxVolume > 0 ? (vol / maxVolume) * 100 : 0;
                  const tokenKey = m.token_id;
                  return (
                    <motion.tr
                      key={m.token_id}
                      initial={{ opacity: 0, x: -8 }}
                      animate={{ opacity: 1, x: 0 }}
                      transition={{ duration: 0.25, delay: i * 0.03 }}
                      onClick={() => navigate(`/market/${encodeURIComponent(m.all_token_ids.join(','))}`)}
                      className="border-b border-[var(--border-subtle)] row-glow cursor-pointer"
                    >
                      <td className={`font-mono text-[var(--text-secondary)] ${compact ? "px-3 py-2" : "px-4 py-3 text-sm"} ${deltaClass(`${tokenKey}:rank`)}`}>{i + 1}</td>
                      <td className={compact ? "px-3 py-2" : "px-4 py-3"}>
                        <div className="flex flex-col gap-1 min-w-0">
                          <span className="text-[var(--text-primary)] truncate max-w-md" title={m.question}>
                            {m.question}
                          </span>
                          {m.outcome && (
                            <motion.span
                              initial={{ scale: 0.8, opacity: 0 }}
                              animate={{ scale: 1, opacity: 1 }}
                              transition={{ duration: 0.2, delay: i * 0.03 + 0.1 }}
                              className="text-xs px-2 py-0.5 rounded-full bg-[var(--accent-orange)]/10 text-[var(--accent-orange)] border border-[var(--accent-orange)]/20 w-fit"
                            >
                              {m.outcome}
                            </motion.span>
                          )}
                        </div>
                      </td>
                      <td className={`${compact ? "px-3 py-2" : "px-4 py-3"} text-right ${deltaClass(`${tokenKey}:volume`)}`}>
                        <div className="flex flex-col items-end gap-1">
                          <span className="font-mono text-[var(--text-primary)]">{formatUsd(m.volume)}</span>
                          <div className="w-24 h-1 rounded-full bg-[var(--border-subtle)] overflow-hidden">
                            <motion.div
                              className="h-full rounded-full bg-gradient-to-r from-blue-500/60 to-orange-500/60"
                              initial={{ width: 0 }}
                              animate={{ width: `${pct}%` }}
                              transition={{ duration: 0.6, ease: "easeOut", delay: i * 0.03 }}
                            />
                          </div>
                        </div>
                      </td>
                      <td className={`${compact ? "px-3 py-2" : "px-4 py-3"} text-right font-mono text-[var(--text-primary)] ${deltaClass(`${tokenKey}:trades`)}`}>
                        {formatNumber(m.trade_count)}
                      </td>
                      <td className={`${compact ? "px-3 py-2" : "px-4 py-3"} text-right font-mono text-[var(--text-secondary)] hidden md:table-cell ${deltaClass(`${tokenKey}:unique`)}`}>
                        {formatNumber(m.unique_traders)}
                      </td>
                      <td className={`${compact ? "px-3 py-2" : "px-4 py-3"} text-left text-[var(--text-secondary)] hidden lg:table-cell`}>
                        {m.category || "\u2014"}
                      </td>
                      <td className={`${compact ? "px-3 py-2" : "px-4 py-3"} text-right text-[var(--text-secondary)] hidden lg:table-cell`}>
                        {timeAgo(m.last_trade)}
                      </td>
                    </motion.tr>
                  );
                })}
              </tbody>
            </table>
          </div>
        </div>
      ) : (
        <div className="glass p-8 text-center text-[var(--text-secondary)]">No market activity found</div>
      )}
    </div>
  );
}
