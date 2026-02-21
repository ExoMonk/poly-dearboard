import { useState } from "react";
import { useParams, Link, useNavigate } from "react-router-dom";
import { useQuery, keepPreviousData } from "@tanstack/react-query";
import { motion } from "motion/react";
import { fetchTrader, fetchTraderTrades, fetchTraderPositions, fetchPnlChart, fetchTraderProfile } from "../api";
import type { OpenPosition, PnlTimeframe, BehavioralLabel } from "../types";
import Spinner from "../components/Spinner";
import Pagination from "../components/Pagination";
import PnlChart from "../charts/PnlChart";
import { formatUsd, formatNumber, formatDate, formatTimestamp, shortenAddress, polygonscanAddress, polygonscanTx, polymarketAddress, formatHoldTime } from "../lib/format";
import { staggerContainer, statCardVariants, tapScale } from "../lib/motion";

const PAGE_SIZE = 50;
type PosTab = "open" | "closed";

export default function TraderDetail() {
  const { address } = useParams<{ address: string }>();
  const [sideFilter, setSideFilter] = useState("");
  const [offset, setOffset] = useState(0);
  const [posTab, setPosTab] = useState<PosTab>("open");
  const [pnlTimeframe, setPnlTimeframe] = useState<PnlTimeframe>("all");

  const { data: trader, isLoading: loadingTrader, error: traderError } = useQuery({
    queryKey: ["trader", address],
    queryFn: () => fetchTrader(address!),
    enabled: !!address,
  });

  const { data: tradesData, isLoading: loadingTrades } = useQuery({
    queryKey: ["trades", address, sideFilter, offset],
    queryFn: () => fetchTraderTrades(address!, { limit: PAGE_SIZE, offset, side: sideFilter || undefined }),
    enabled: !!address,
    placeholderData: keepPreviousData,
  });

  const { data: positionsData } = useQuery({
    queryKey: ["positions", address],
    queryFn: () => fetchTraderPositions(address!),
    enabled: !!address,
  });

  const { data: pnlChart } = useQuery({
    queryKey: ["pnlChart", address, pnlTimeframe],
    queryFn: () => fetchPnlChart(address!, pnlTimeframe),
    enabled: !!address,
  });

  const { data: profile } = useQuery({
    queryKey: ["trader-profile", address],
    queryFn: () => fetchTraderProfile(address!),
    enabled: !!address,
  });

  if (loadingTrader) return <Spinner />;
  if (traderError) return <div className="text-[var(--neon-red)] text-center py-10">Trader not found</div>;
  if (!trader) return null;

  const pnl = parseFloat(trader.realized_pnl);

  return (
    <div className="space-y-8">
      {/* Header */}
      <div>
        <motion.div whileHover={{ x: -4 }} className="inline-block">
          <Link to="/" className="text-sm text-[var(--text-secondary)] hover:text-[var(--accent-blue)] transition-colors duration-200">
            &larr; Back to Leaderboard
          </Link>
        </motion.div>
        <div className="flex items-center gap-3 mt-3">
          <h1 className="text-2xl font-black font-mono gradient-text">{shortenAddress(address!)}</h1>
          <button
            onClick={() => navigator.clipboard.writeText(address!)}
            className="text-[var(--text-secondary)] hover:text-[var(--accent-blue)] transition-colors duration-200"
            title="Copy address"
          >
            <svg className="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M8 16H6a2 2 0 01-2-2V6a2 2 0 012-2h8a2 2 0 012 2v2m-6 12h8a2 2 0 002-2v-8a2 2 0 00-2-2h-8a2 2 0 00-2 2v8a2 2 0 002 2z" />
            </svg>
          </button>
          <a
            href={polygonscanAddress(address!)}
            target="_blank"
            rel="noopener noreferrer"
            className="text-[var(--text-secondary)] hover:text-[var(--accent-blue)] transition-colors duration-200 text-sm"
          >
            Polygonscan ↗
          </a>
          <a
            href={polymarketAddress(address!)}
            target="_blank"
            rel="noopener noreferrer"
            className="text-[var(--text-secondary)] hover:text-[var(--accent-blue)] transition-colors duration-200 text-sm"
          >
            Polymarket ↗
          </a>
        </div>
        {profile && profile.labels.length > 0 && (
          <div className="flex flex-wrap gap-2 mt-2">
            {profile.labels.map((label) => (
              <LabelBadge key={label} label={label} />
            ))}
          </div>
        )}
      </div>

      {/* Stats Cards */}
      <motion.div
        className="grid grid-cols-2 md:grid-cols-4 gap-4"
        variants={staggerContainer}
        initial="initial"
        animate="animate"
      >
        <StatCard label="PnL" value={formatUsd(trader.realized_pnl)} glow={pnl >= 0 ? "green" : "red"} />
        <StatCard label="Total Volume" value={formatUsd(trader.total_volume)} />
        <StatCard label="Trades" value={formatNumber(trader.trade_count)} />
        <StatCard label="Markets" value={formatNumber(trader.markets_traded)} />
        {profile ? (
          <>
            <StatCard label="Avg Position" value={formatUsd(profile.avg_position_size)} />
            <StatCard label="Avg Hold Time" value={formatHoldTime(profile.avg_hold_time_hours)} />
            <StatCard
              label="Biggest Win"
              value={profile.biggest_win ? formatUsd(profile.biggest_win.pnl) : "—"}
              glow={profile.biggest_win ? "green" : undefined}
              subtitle={profile.biggest_win?.question}
            />
            <StatCard
              label="Biggest Loss"
              value={profile.biggest_loss ? formatUsd(profile.biggest_loss.pnl) : "—"}
              glow={profile.biggest_loss ? "red" : undefined}
              subtitle={profile.biggest_loss?.question}
            />
          </>
        ) : (
          <>
            <StatCard label="Total Fees" value={formatUsd(trader.total_fees)} />
            <StatCard label="Active Since" value={formatDate(trader.first_trade)} />
          </>
        )}
      </motion.div>

      {/* Win Rate & Z-Score (if profile available with resolved positions) */}
      {profile && profile.label_details.settled_count > 0 && (
        <motion.div
          className="grid grid-cols-2 md:grid-cols-4 gap-4"
          variants={staggerContainer}
          initial="initial"
          animate="animate"
        >
          <StatCard
            label="Win Rate"
            value={`${profile.label_details.win_rate.toFixed(1)}%`}
            glow={profile.label_details.win_rate > 55 ? "green" : profile.label_details.win_rate < 45 ? "red" : undefined}
          />
          <StatCard label="Z-Score" value={profile.label_details.z_score.toFixed(2)} />
          <StatCard label="Settled" value={`${profile.label_details.settled_count} / ${profile.total_positions}`} />
          <StatCard label="Active Span" value={formatHoldTime(profile.label_details.active_span_days * 24)} />
        </motion.div>
      )}

      {/* Category Breakdown */}
      {profile && profile.category_breakdown.length > 0 && (
        <div>
          <h2 className="text-lg font-bold gradient-text mb-4">Category Breakdown</h2>
          <div className="glass p-4 space-y-3">
            {profile.category_breakdown.map((cat) => {
              const vol = parseFloat(cat.volume);
              const totalVol = parseFloat(profile.label_details.total_volume);
              const pct = totalVol > 0 ? (vol / totalVol) * 100 : 0;
              const catPnl = parseFloat(cat.pnl);
              return (
                <div key={cat.category}>
                  <div className="flex items-center justify-between text-sm mb-1">
                    <span className="text-[var(--text-primary)] font-medium">{cat.category || "Unknown"}</span>
                    <div className="flex items-center gap-4 text-xs">
                      <span className="text-[var(--text-secondary)]">{cat.trade_count} trades</span>
                      <span className="text-[var(--text-secondary)]">{formatUsd(cat.volume)}</span>
                      <span className={catPnl >= 0 ? "glow-green" : "glow-red"}>{formatUsd(cat.pnl)}</span>
                    </div>
                  </div>
                  <div className="w-full bg-[var(--bg-tertiary)] rounded-full h-2">
                    <div
                      className="h-2 rounded-full transition-all duration-500"
                      style={{
                        width: `${Math.min(pct, 100)}%`,
                        background: catPnl >= 0
                          ? "var(--neon-green)"
                          : "var(--neon-red)",
                        opacity: 0.7,
                      }}
                    />
                  </div>
                </div>
              );
            })}
          </div>
        </div>
      )}

      {/* PnL Chart */}
      {pnlChart && (
        <PnlChart
          points={pnlChart.points}
          timeframe={pnlTimeframe}
          onTimeframeChange={setPnlTimeframe}
        />
      )}

      {/* Positions */}
      {positionsData && (positionsData.open.length > 0 || positionsData.closed.length > 0) && (
        <div>
          <div className="flex items-center gap-4 mb-4">
            <h2 className="text-lg font-bold gradient-text">Positions</h2>
            <div className="flex gap-1">
              {([
                { value: "open" as PosTab, label: "Open", count: positionsData.open.length },
                { value: "closed" as PosTab, label: "Closed", count: positionsData.closed.length },
              ]).map((tab) => (
                <motion.button
                  key={tab.value}
                  onClick={() => setPosTab(tab.value)}
                  whileTap={tapScale}
                  className={`px-4 py-1.5 text-xs rounded-full font-medium transition-all duration-200 ${
                    posTab === tab.value
                      ? "bg-[var(--accent-blue)]/10 text-[var(--accent-blue)] border border-[var(--accent-blue)]/30 shadow-[0_0_8px_rgba(59,130,246,0.15)]"
                      : "text-[var(--text-secondary)] border border-transparent hover:text-[var(--text-primary)] hover:border-[var(--border-glow)]"
                  }`}
                >
                  {tab.label} ({tab.count})
                </motion.button>
              ))}
            </div>
          </div>
          <PositionsTable positions={posTab === "open" ? positionsData.open : positionsData.closed} />
        </div>
      )}

      {/* Trades Table */}
      <div>
        <div className="flex items-center justify-between mb-4">
          <h2 className="text-lg font-bold gradient-text">Trade History</h2>
          <div className="flex gap-1">
            {["", "buy", "sell"].map((s) => (
              <motion.button
                key={s}
                onClick={() => { setSideFilter(s); setOffset(0); }}
                whileTap={tapScale}
                className={`px-4 py-1.5 text-xs rounded-full font-medium transition-all duration-200 ${
                  sideFilter === s
                    ? "bg-[var(--accent-blue)]/10 text-[var(--accent-blue)] border border-[var(--accent-blue)]/30 shadow-[0_0_8px_rgba(59,130,246,0.15)]"
                    : "text-[var(--text-secondary)] border border-transparent hover:text-[var(--text-primary)] hover:border-[var(--border-glow)]"
                }`}
              >
                {s === "" ? "All" : s === "buy" ? "Buy" : "Sell"}
              </motion.button>
            ))}
          </div>
        </div>

        {loadingTrades ? (
          <Spinner />
        ) : tradesData && tradesData.trades.length > 0 ? (
          <>
            <div className="glass overflow-hidden">
              <div className="overflow-x-auto">
                <table className="w-full text-sm">
                  <thead>
                    <tr className="border-b border-[var(--border-glow)] text-[var(--text-secondary)] text-xs uppercase tracking-widest">
                      <th className="px-4 py-3 text-left">Time</th>
                      <th className="px-4 py-3 text-right">Block</th>
                      <th className="px-4 py-3 text-center">Side</th>
                      <th className="px-4 py-3 text-right">Amount</th>
                      <th className="px-4 py-3 text-right">Price</th>
                      <th className="px-4 py-3 text-right">USDC</th>
                      <th className="px-4 py-3 text-right">Fee</th>
                      <th className="px-4 py-3 text-right">Tx</th>
                    </tr>
                  </thead>
                  <tbody>
                    {tradesData.trades.map((t, i) => (
                      <motion.tr
                        key={`${t.tx_hash}-${i}`}
                        initial={{ opacity: 0, x: -8 }}
                        animate={{ opacity: 1, x: 0 }}
                        transition={{ duration: 0.25, delay: i * 0.02 }}
                        className="border-b border-[var(--border-subtle)] row-glow"
                      >
                        <td className="px-4 py-3 text-[var(--text-secondary)] whitespace-nowrap">{formatTimestamp(t.block_timestamp)}</td>
                        <td className="px-4 py-3 text-right font-mono text-[var(--text-secondary)] text-xs">{formatNumber(t.block_number)}</td>
                        <td className="px-4 py-3 text-center">
                          <span className={`text-xs font-bold px-2.5 py-0.5 rounded-full ${
                            t.side === "buy"
                              ? "bg-[var(--neon-green)]/10 text-[var(--neon-green)] shadow-[0_0_6px_rgba(0,255,136,0.15)]"
                              : "bg-[var(--neon-red)]/10 text-[var(--neon-red)] shadow-[0_0_6px_rgba(255,51,102,0.15)]"
                          }`}>
                            {t.side.toUpperCase()}
                          </span>
                        </td>
                        <td className="px-4 py-3 text-right font-mono text-[var(--text-primary)]">{formatNumber(t.amount)}</td>
                        <td className="px-4 py-3 text-right font-mono text-[var(--text-primary)]">{formatUsd(t.price)}</td>
                        <td className="px-4 py-3 text-right font-mono text-[var(--text-primary)]">{formatUsd(t.usdc_amount)}</td>
                        <td className="px-4 py-3 text-right font-mono text-[var(--text-secondary)]">{formatUsd(t.fee)}</td>
                        <td className="px-4 py-3 text-right">
                          <a
                            href={polygonscanTx(t.tx_hash)}
                            target="_blank"
                            rel="noopener noreferrer"
                            className="text-[var(--accent-blue)]/50 hover:text-[var(--accent-blue)] font-mono text-xs transition-colors duration-200"
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
            <Pagination total={tradesData.total} limit={PAGE_SIZE} offset={offset} onPageChange={setOffset} />
          </>
        ) : (
          <p className="text-[var(--text-secondary)] text-center py-8">No trades found</p>
        )}
      </div>
    </div>
  );
}

function PositionsTable({ positions }: { positions: OpenPosition[] }) {
  const navigate = useNavigate();

  if (positions.length === 0) {
    return <p className="text-[var(--text-secondary)] text-center py-8">No positions</p>;
  }
  return (
    <div className="glass overflow-hidden">
      <div className="overflow-x-auto">
        <table className="w-full text-sm">
          <thead>
            <tr className="border-b border-[var(--border-glow)] text-[var(--text-secondary)] text-xs uppercase tracking-widest">
              <th className="px-4 py-3 text-left">Market</th>
              <th className="px-4 py-3 text-center">Outcome</th>
              <th className="px-4 py-3 text-center">Side</th>
              <th className="px-4 py-3 text-right">Tokens</th>
              <th className="px-4 py-3 text-right">Avg Cost</th>
              <th className="px-4 py-3 text-right">Price</th>
              <th className="px-4 py-3 text-right">PnL</th>
              <th className="px-4 py-3 text-right">Volume</th>
            </tr>
          </thead>
          <tbody>
            {positions.map((p, i) => {
              const positionPnl = parseFloat(p.pnl);
              return (
                <motion.tr
                  key={`${p.asset_id}-${i}`}
                  initial={{ opacity: 0, x: -8 }}
                  animate={{ opacity: 1, x: 0 }}
                  transition={{ duration: 0.25, delay: i * 0.02 }}
                  className="border-b border-[var(--border-subtle)] row-glow cursor-pointer"
                  onClick={() => navigate(`/market/${p.asset_id}`)}
                >
                  <td className="px-4 py-3 max-w-xs truncate" title={p.question}>
                    <span className="text-[var(--text-primary)] hover:text-[var(--accent-cyan)] transition-colors duration-200">
                      {p.question}
                    </span>
                  </td>
                  <td className="px-4 py-3 text-center">
                    {p.outcome && (
                      <span className={`text-xs font-bold px-2.5 py-0.5 rounded-full ${
                        p.outcome.toLowerCase() === "yes"
                          ? "bg-[var(--neon-green)]/10 text-[var(--neon-green)]"
                          : "bg-[var(--neon-red)]/10 text-[var(--neon-red)]"
                      }`}>
                        {p.outcome}
                      </span>
                    )}
                  </td>
                  <td className="px-4 py-3 text-center">
                    <span className={`text-xs font-bold px-2.5 py-0.5 rounded-full ${
                      p.side === "long"
                        ? "bg-[var(--neon-green)]/10 text-[var(--neon-green)] shadow-[0_0_6px_rgba(0,255,136,0.15)]"
                        : p.side === "short"
                        ? "bg-[var(--neon-red)]/10 text-[var(--neon-red)] shadow-[0_0_6px_rgba(255,51,102,0.15)]"
                        : "bg-[var(--text-secondary)]/10 text-[var(--text-secondary)]"
                    }`}>
                      {p.side.toUpperCase()}
                    </span>
                  </td>
                  <td className="px-4 py-3 text-right font-mono text-[var(--text-primary)]">
                    {formatNumber(Math.abs(parseFloat(p.net_tokens)))}
                  </td>
                  <td className="px-4 py-3 text-right font-mono text-[var(--text-secondary)]">
                    {formatUsd(p.cost_basis)}
                  </td>
                  <td className="px-4 py-3 text-right font-mono text-[var(--text-primary)]">
                    {formatUsd(p.latest_price)}
                  </td>
                  <td className={`px-4 py-3 text-right font-mono ${positionPnl >= 0 ? "glow-green" : "glow-red"}`}>
                    {formatUsd(p.pnl)}
                  </td>
                  <td className="px-4 py-3 text-right font-mono text-[var(--text-secondary)]">
                    {formatUsd(p.volume)}
                  </td>
                </motion.tr>
              );
            })}
          </tbody>
        </table>
      </div>
    </div>
  );
}

function StatCard({ label, value, glow, subtitle }: { label: string; value: string; glow?: "green" | "red"; subtitle?: string }) {
  const borderColor = glow === "green"
    ? "border-[var(--neon-green)]/20 shadow-[0_0_12px_rgba(0,255,136,0.08)]"
    : glow === "red"
    ? "border-[var(--neon-red)]/20 shadow-[0_0_12px_rgba(255,51,102,0.08)]"
    : "";

  const valueClass = glow === "green"
    ? "glow-green"
    : glow === "red"
    ? "glow-red"
    : "text-[var(--text-primary)]";

  return (
    <motion.div variants={statCardVariants} className={`glass p-4 gradient-border-top ${borderColor}`}>
      <div className="text-xs text-[var(--text-secondary)] mb-2 uppercase tracking-wider">{label}</div>
      <div className={`text-xl font-bold font-mono ${valueClass}`}>{value}</div>
      {subtitle && (
        <div className="text-xs text-[var(--text-secondary)] mt-1 truncate" title={subtitle}>{subtitle}</div>
      )}
    </motion.div>
  );
}

const LABEL_STYLES: Record<BehavioralLabel, { text: string; bg: string; border: string; glow: string }> = {
  sharp: { text: "Sharp", bg: "bg-emerald-500/10", border: "border-emerald-500/30", glow: "shadow-[0_0_8px_rgba(16,185,129,0.2)]" },
  specialist: { text: "Specialist", bg: "bg-cyan-500/10", border: "border-cyan-500/30", glow: "shadow-[0_0_8px_rgba(6,182,212,0.2)]" },
  whale: { text: "Whale", bg: "bg-blue-500/10", border: "border-blue-500/30", glow: "shadow-[0_0_8px_rgba(59,130,246,0.2)]" },
  degen: { text: "Degen", bg: "bg-orange-500/10", border: "border-orange-500/30", glow: "shadow-[0_0_8px_rgba(249,115,22,0.2)]" },
  market_maker: { text: "Market Maker", bg: "bg-purple-500/10", border: "border-purple-500/30", glow: "shadow-[0_0_8px_rgba(168,85,247,0.2)]" },
  bot: { text: "Bot", bg: "bg-yellow-500/10", border: "border-yellow-500/30", glow: "shadow-[0_0_8px_rgba(234,179,8,0.2)]" },
  casual: { text: "Casual", bg: "bg-gray-500/10", border: "border-gray-500/30", glow: "" },
};

function LabelBadge({ label }: { label: BehavioralLabel }) {
  const style = LABEL_STYLES[label];
  return (
    <span className={`text-xs font-bold px-3 py-1 rounded-full border ${style.bg} ${style.border} ${style.glow}`}>
      {style.text}
    </span>
  );
}
