import { useState, useEffect, useMemo, useRef } from "react";
import { useQuery, keepPreviousData } from "@tanstack/react-query";
import { motion } from "motion/react";
import { fetchLeaderboard } from "../api";
import type { SortColumn, SortOrder, Timeframe } from "../types";
import Spinner from "../components/Spinner";
import Pagination from "../components/Pagination";
import AddressCell from "../components/AddressCell";
import SortHeader from "../components/SortHeader";
import LabelBadge from "../components/LabelBadge";
import { labelTooltip } from "../lib/labels";
import SmartMoney from "../charts/SmartMoney";
import AddToListButton from "../components/AddToListButton";
import { formatUsd, formatNumber, formatDate, timeAgo } from "../lib/format";
import { tapScale } from "../lib/motion";
import { freshnessLabel } from "../lib/streamControls";

const PAGE_SIZE = 25;

const TIMEFRAMES = [
  { label: "1H", value: "1h" },
  { label: "24H", value: "24h" },
  { label: "All", value: "all" },
] as const;

function rankClass(rank: number): string {
  if (rank === 1) return "rank-gold font-bold";
  if (rank === 2) return "rank-silver font-bold";
  if (rank === 3) return "rank-bronze font-bold";
  return "text-[var(--text-secondary)]";
}

export default function Dashboard() {
  const [sort, setSort] = useState<SortColumn>("realized_pnl");
  const [order, setOrder] = useState<SortOrder>("desc");
  const [offset, setOffset] = useState(0);
  const [timeframe, setTimeframe] = useState<Timeframe>("24h");
  const [deltaByKey, setDeltaByKey] = useState<Record<string, "positive" | "negative" | "neutral">>({});
  const [lastUpdatedAt, setLastUpdatedAt] = useState<number | null>(null);
  const [nowTs, setNowTs] = useState(() => Date.now());
  const prevSnapshotRef = useRef<Record<string, { rank: number; realizedPnl: number; volume: number; trades: number }>>({});

  const { data, isLoading, error } = useQuery({
    queryKey: ["leaderboard", sort, order, offset, timeframe],
    queryFn: () => fetchLeaderboard({ sort, order, limit: PAGE_SIZE, offset, timeframe }),
    placeholderData: keepPreviousData,
  });

  useEffect(() => {
    const timer = window.setInterval(() => setNowTs(Date.now()), 1000);
    return () => window.clearInterval(timer);
  }, []);

  useEffect(() => {
    if (!data) return;
    setLastUpdatedAt(Date.now());

    const prev = prevSnapshotRef.current;
    const next: Record<string, { rank: number; realizedPnl: number; volume: number; trades: number }> = {};
    const changes: Record<string, "positive" | "negative" | "neutral"> = {};

    data.traders.forEach((trader, index) => {
      const address = trader.address.toLowerCase();
      const rank = offset + index + 1;
      const current = {
        rank,
        realizedPnl: Number.parseFloat(trader.realized_pnl),
        volume: Number.parseFloat(trader.total_volume),
        trades: trader.trade_count,
      };
      next[address] = current;

      const previous = prev[address];
      if (!previous) {
        if (Object.keys(prev).length > 0) {
          changes[`${address}:rank`] = "neutral";
        }
        return;
      }

      if (current.rank !== previous.rank) {
        changes[`${address}:rank`] = current.rank < previous.rank ? "positive" : "negative";
      }
      if (current.realizedPnl !== previous.realizedPnl) {
        changes[`${address}:pnl`] = current.realizedPnl > previous.realizedPnl ? "positive" : "negative";
      }
      if (current.volume !== previous.volume) {
        changes[`${address}:volume`] = current.volume > previous.volume ? "positive" : "negative";
      }
      if (current.trades !== previous.trades) {
        changes[`${address}:trades`] = current.trades > previous.trades ? "positive" : "negative";
      }
    });

    prevSnapshotRef.current = next;
    if (Object.keys(changes).length > 0) {
      setDeltaByKey(changes);
      const timer = window.setTimeout(() => setDeltaByKey({}), 2000);
      return () => window.clearTimeout(timer);
    }
  }, [data, offset]);

  const freshness = useMemo(() => freshnessLabel(lastUpdatedAt, nowTs), [lastUpdatedAt, nowTs]);
  const freshnessDisplay = freshness === "Stale" ? "Refreshing..." : freshness;

  const deltaClass = (key: string): string => {
    const kind = deltaByKey[key];
    if (kind === "positive") return "delta-flash-positive";
    if (kind === "negative") return "delta-flash-negative";
    if (kind === "neutral") return "delta-flash-neutral";
    return "";
  };

  function handleSort(col: SortColumn) {
    if (col === sort) {
      setOrder(order === "desc" ? "asc" : "desc");
    } else {
      setSort(col);
      setOrder("desc");
    }
    setOffset(0);
  }

  if (isLoading) return <Spinner />;
  if (error) return <div className="text-[var(--neon-red)] text-center py-10">Failed to load leaderboard</div>;
  if (!data) return null;

  return (
    <div className="space-y-8">
      {/* Header */}
      <div className="flex items-center justify-between">
        <div className="flex items-center gap-4">
          <h1 className="text-3xl font-black gradient-text tracking-tight glitch-text">Leaderboard</h1>
          <div className="flex gap-1">
            {TIMEFRAMES.map((tf) => (
              <motion.button
                key={tf.value}
                onClick={() => { setTimeframe(tf.value); setOffset(0); }}
                whileTap={tapScale}
                className={`px-4 py-1.5 text-xs rounded-full font-medium transition-all duration-200 ${
                  timeframe === tf.value
                    ? "bg-[var(--accent-blue)]/10 text-[var(--accent-blue)] border border-[var(--accent-blue)]/30 shadow-[0_0_8px_rgba(59,130,246,0.15)]"
                    : "text-[var(--text-secondary)] border border-transparent hover:text-[var(--text-primary)] hover:border-[var(--border-glow)]"
                }`}
              >
                {tf.label}
              </motion.button>
            ))}
          </div>
        </div>
        <div className="flex items-center gap-2">
          <span className="text-sm text-[var(--text-secondary)] font-mono">{data.total.toLocaleString()} traders</span>
          <span
            className={`text-xs px-2 py-1 rounded-full ${
              freshness === "Updated <10s"
                ? "text-[var(--neon-green)] bg-[var(--neon-green)]/10"
                : "text-[var(--accent-orange)] bg-[var(--accent-orange)]/10"
            }`}
          >
            {freshnessDisplay}
          </span>
        </div>
      </div>

      {/* Chart */}
      <SmartMoney timeframe={timeframe} />

      {/* Table */}
      <div className="glass overflow-hidden">
        <div className="overflow-x-auto">
          <table className="w-full text-sm">
            <thead>
              <tr className="border-b border-[var(--border-glow)] text-[var(--text-secondary)] text-xs uppercase tracking-widest">
                <th className="px-4 py-3 text-left w-14">#</th>
                <th className="px-4 py-3 text-left">Trader</th>
                <th className="px-4 py-3 text-left hidden md:table-cell">Labels</th>
                <SortHeader label="PnL" column="realized_pnl" currentSort={sort} currentOrder={order} onSort={handleSort} />
                <SortHeader label="Volume" column="total_volume" currentSort={sort} currentOrder={order} onSort={handleSort} />
                <SortHeader label="Trades" column="trade_count" currentSort={sort} currentOrder={order} onSort={handleSort} />
                <th className="px-4 py-3 text-right">Markets</th>
                <th className="px-4 py-3 text-right hidden lg:table-cell">First</th>
                <th className="px-4 py-3 text-right hidden lg:table-cell">Last</th>
                <th className="px-4 py-3 w-10"></th>
              </tr>
            </thead>
            <tbody>
              {data.traders.map((t, i) => {
                const rank = offset + i + 1;
                const pnl = parseFloat(t.realized_pnl);
                const addrKey = t.address.toLowerCase();
                return (
                  <motion.tr
                    key={t.address}
                    initial={{ opacity: 0, x: -8 }}
                    animate={{ opacity: 1, x: 0 }}
                    transition={{ duration: 0.25, delay: i * 0.03 }}
                    className="border-b border-[var(--border-subtle)] row-glow"
                  >
                    <td className={`px-4 py-3 font-mono text-sm ${rankClass(rank)} ${deltaClass(`${addrKey}:rank`)}`}>{rank}</td>
                    <td className="px-4 py-3">
                      <AddressCell address={t.address} />
                    </td>
                    <td className="px-4 py-3 hidden md:table-cell">
                      <div className="flex flex-wrap gap-1">
                        {(data.labels[t.address.toLowerCase()] || []).map((label) => {
                          const details = data.label_details[t.address.toLowerCase()];
                          return (
                            <LabelBadge
                              key={label}
                              label={label}
                              size="sm"
                              tooltip={details ? labelTooltip(label, details) : undefined}
                            />
                          );
                        })}
                      </div>
                    </td>
                    <td className={`px-4 py-3 text-right font-mono ${pnl >= 0 ? "glow-green" : "glow-red"} ${deltaClass(`${addrKey}:pnl`)}`}>
                      {formatUsd(t.realized_pnl)}
                    </td>
                    <td className={`px-4 py-3 text-right font-mono text-[var(--text-primary)] ${deltaClass(`${addrKey}:volume`)}`}>{formatUsd(t.total_volume)}</td>
                    <td className={`px-4 py-3 text-right font-mono text-[var(--text-primary)] ${deltaClass(`${addrKey}:trades`)}`}>{formatNumber(t.trade_count)}</td>
                    <td className="px-4 py-3 text-right text-[var(--text-secondary)]">{formatNumber(t.markets_traded)}</td>
                    <td className="px-4 py-3 text-right text-[var(--text-secondary)] hidden lg:table-cell">{formatDate(t.first_trade)}</td>
                    <td className="px-4 py-3 text-right text-[var(--text-secondary)] hidden lg:table-cell">{timeAgo(t.last_trade)}</td>
                    <td className="px-4 py-3 text-center">
                      <AddToListButton address={t.address} />
                    </td>
                  </motion.tr>
                );
              })}
            </tbody>
          </table>
        </div>
      </div>

      <Pagination total={data.total} limit={PAGE_SIZE} offset={offset} onPageChange={setOffset} />
    </div>
  );
}
