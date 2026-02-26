import { useState } from "react";
import { Link } from "react-router-dom";
import { useQuery } from "@tanstack/react-query";
import { motion } from "motion/react";
import {
  AreaChart,
  Area,
  XAxis,
  YAxis,
  Tooltip,
  ReferenceLine,
  ResponsiveContainer,
  CartesianGrid,
} from "recharts";
import { fetchBacktest } from "../../api";
import type { BacktestTimeframe } from "../../types";
import { formatUsd, formatNumber, shortenAddress } from "../../lib/format";
import {
  staggerContainer,
  statCardVariants,
  panelVariants,
  tableRowVariants,
  hoverGlow,
} from "../../lib/motion";
import { useDebounce } from "../../hooks/useDebounce";
import Spinner from "../../components/Spinner";
import { Pill, SectionHeader, rankClass, TOP_N_OPTIONS, TOOLTIP_STYLE } from "./shared";
import ListSelector from "./ListSelector";

const TIMEFRAMES: { value: BacktestTimeframe; label: string }[] = [
  { value: "7d", label: "7D" },
  { value: "30d", label: "30D" },
  { value: "all", label: "All" },
];
const COPY_PCT_OPTIONS = [
  { value: 0.05, label: "5%" },
  { value: 0.1, label: "10%" },
  { value: 0.25, label: "25%" },
  { value: 0.5, label: "50%" },
  { value: 1.0, label: "100%" },
];
const CHART_MODES = [
  { key: "value" as const, label: "Portfolio Value" },
  { key: "pnl" as const, label: "P&L" },
  { key: "pnl_pct" as const, label: "Return %" },
];

function formatDateLabel(dateStr: string): string {
  const d = new Date(dateStr);
  if (isNaN(d.getTime())) return dateStr;
  return d.toLocaleDateString("en-US", { month: "short", day: "numeric" });
}

type ChartMode = "value" | "pnl" | "pnl_pct";

export default function Backtester() {
  const [topN, setTopN] = useState<number>(10);
  const [listId, setListId] = useState<string | null>(null);
  const [timeframe, setTimeframe] = useState<BacktestTimeframe>("30d");
  const [capital, setCapital] = useState<number>(10000);
  const [capitalInput, setCapitalInput] = useState<string>("10000");
  const [copyPct, setCopyPct] = useState<number>(1.0);
  const [chartMode, setChartMode] = useState<ChartMode>("value");

  const debouncedCapital = useDebounce(capital, 500);

  const { data, isLoading, isError } = useQuery({
    queryKey: ["backtest", listId, topN, timeframe, debouncedCapital, copyPct],
    queryFn: () =>
      fetchBacktest({
        topN: listId ? undefined : topN,
        listId: listId ?? undefined,
        timeframe,
        initialCapital: debouncedCapital,
        copyPct,
      }),
    staleTime: 60_000,
  });

  const traderCount = listId ? (data?.summary.traders_count ?? 1) : topN;
  const perTraderBudget = (debouncedCapital * copyPct) / traderCount;
  const initialCap = data?.config.initial_capital ?? debouncedCapital;

  const chartData = (data?.portfolio_curve ?? []).map((p) => ({
    date: formatDateLabel(p.date),
    value: parseFloat(p.value),
    pnl: parseFloat(p.pnl),
    pnl_pct: parseFloat(p.pnl_pct),
  }));

  const dataKey = chartMode;
  const values = chartData.map((d) => d[dataKey]);
  const refValue = chartMode === "value" ? initialCap : 0;
  const maxVal = Math.max(...values, refValue);
  const minVal = Math.min(...values, refValue);
  const range = maxVal - minVal;
  const zeroOffset = range > 0 ? (maxVal - refValue) / range : 0.5;

  const tooltipLabel = chartMode === "value" ? "Value" : chartMode === "pnl" ? "P&L" : "Return %";
  const tooltipFormatter = (v: number | undefined) => {
    if (chartMode === "pnl_pct") return [`${(v ?? 0).toFixed(2)}%`, tooltipLabel];
    return [formatUsd(String(v ?? 0)), tooltipLabel];
  };
  const yFormatter = (v: number) => {
    if (chartMode === "pnl_pct") return `${v.toFixed(1)}%`;
    return formatUsd(String(v));
  };

  const totalPnl = parseFloat(data?.summary.total_pnl ?? "0");

  return (
    <>
      {/* Strategy Config */}
      <motion.div
        variants={panelVariants}
        initial="initial"
        animate="animate"
        className="glass p-6 gradient-border-top"
      >
        <SectionHeader dot="bg-[var(--accent-blue)] shadow-[0_0_6px_var(--accent-blue)]">
          Strategy Configuration
        </SectionHeader>

        <div className="flex flex-col gap-5">
          {/* Row 1: Capital + Copy % */}
          <div className="flex flex-wrap items-center gap-5">
            <div className="flex items-center gap-2.5">
              <span className="text-xs font-medium text-[var(--text-secondary)] uppercase tracking-wider">Capital</span>
              <div className="relative group">
                <span className="absolute left-3 top-1/2 -translate-y-1/2 text-[var(--accent-blue)] text-sm font-mono font-bold">$</span>
                <input
                  type="number"
                  value={capitalInput}
                  onChange={(e) => {
                    setCapitalInput(e.target.value);
                    const n = Number(e.target.value);
                    if (n >= 100) setCapital(n);
                  }}
                  onBlur={() => {
                    const clamped = Math.min(Math.max(100, Number(capitalInput) || 100), 1000000);
                    setCapital(clamped);
                    setCapitalInput(String(clamped));
                  }}
                  className="w-36 pl-7 pr-3 py-2 text-sm font-mono rounded-lg bg-[var(--bg-deep)] border border-[var(--border-glow)] text-[var(--text-primary)] focus:border-[var(--accent-blue)] focus:outline-none focus:shadow-[0_0_12px_rgba(59,130,246,0.2)] transition-all"
                  min={100}
                  max={1000000}
                  step={1000}
                />
                <div className="absolute inset-0 rounded-lg pointer-events-none opacity-0 group-hover:opacity-100 transition-opacity shadow-[0_0_12px_rgba(59,130,246,0.1)]" />
              </div>
            </div>
            <div className="h-5 w-px bg-[var(--border-glow)] hidden sm:block" />
            <div className="flex items-center gap-2">
              <span className="text-xs font-medium text-[var(--text-secondary)] uppercase tracking-wider">Copy %</span>
              <div className="flex gap-1.5">
                {COPY_PCT_OPTIONS.map((opt) => (
                  <Pill key={opt.value} active={copyPct === opt.value} onClick={() => setCopyPct(opt.value)}>
                    {opt.label}
                  </Pill>
                ))}
              </div>
            </div>
          </div>

          {/* Row 2: Source + Top N + Timeframe + Budget readout */}
          <div className="flex flex-wrap items-center gap-5">
            <ListSelector selectedId={listId} onSelect={setListId} />
            {!listId && (
              <>
                <div className="h-5 w-px bg-[var(--border-glow)] hidden sm:block" />
                <div className="flex items-center gap-2">
                  <span className="text-xs font-medium text-[var(--text-secondary)] uppercase tracking-wider">Top</span>
                  <div className="flex gap-1.5">
                    {TOP_N_OPTIONS.map((n) => (
                      <Pill key={n} active={topN === n} onClick={() => setTopN(n)}>
                        {n}
                      </Pill>
                    ))}
                  </div>
                </div>
              </>
            )}
            <div className="h-5 w-px bg-[var(--border-glow)] hidden sm:block" />
            <div className="flex items-center gap-2">
              <span className="text-xs font-medium text-[var(--text-secondary)] uppercase tracking-wider">Period</span>
              <div className="flex gap-1.5">
                {TIMEFRAMES.map((tf) => (
                  <Pill key={tf.value} active={timeframe === tf.value} onClick={() => setTimeframe(tf.value)}>
                    {tf.label}
                  </Pill>
                ))}
              </div>
            </div>
            <div className="ml-auto flex items-center gap-2 bg-[var(--bg-deep)] px-3 py-1.5 rounded-lg border border-[var(--border-glow)]">
              <span className="text-[10px] uppercase tracking-wider text-[var(--text-secondary)]">Per trader</span>
              <span className="font-mono font-bold text-sm text-[var(--accent-blue)]">
                {formatUsd(String(perTraderBudget))}
              </span>
            </div>
          </div>
        </div>
      </motion.div>

      {/* Portfolio Chart */}
      <motion.div
        variants={panelVariants}
        initial="initial"
        animate="animate"
        transition={{ delay: 0.1 }}
        className="glass p-6"
      >
        <div className="flex items-center justify-between mb-5">
          <div className="flex gap-1.5">
            {CHART_MODES.map((m) => (
              <Pill key={m.key} active={chartMode === m.key} onClick={() => setChartMode(m.key)}>
                {m.label}
              </Pill>
            ))}
          </div>
          {data && (
            <div className="flex items-center gap-4">
              <span className={`font-mono font-black text-xl ${totalPnl >= 0 ? "glow-green" : "glow-red"}`}>
                {formatUsd(data.summary.total_pnl)}
              </span>
              <span
                className={`font-mono text-sm px-2 py-0.5 rounded-full ${
                  data.summary.total_return_pct >= 0
                    ? "text-[var(--neon-green)] bg-[var(--neon-green)]/10"
                    : "text-[var(--neon-red)] bg-[var(--neon-red)]/10"
                }`}
              >
                {data.summary.total_return_pct >= 0 ? "+" : ""}
                {data.summary.total_return_pct.toFixed(1)}%
              </span>
            </div>
          )}
        </div>
        {isLoading ? (
          <Spinner />
        ) : isError ? (
          <p className="text-[var(--neon-red)] text-center py-16 text-sm">Failed to load backtest</p>
        ) : chartData.length === 0 ? (
          <p className="text-[var(--text-secondary)] text-center py-16 text-sm">No data for this configuration</p>
        ) : (
          <ResponsiveContainer width="100%" height={320}>
            <AreaChart data={chartData} margin={{ left: 10, right: 10, top: 10, bottom: 0 }}>
              <defs>
                <linearGradient id="labFill" x1="0" y1="0" x2="0" y2="1">
                  <stop offset={0} stopColor="#00ff88" stopOpacity={0.25} />
                  <stop offset={zeroOffset} stopColor="#00ff88" stopOpacity={0.03} />
                  <stop offset={zeroOffset} stopColor="#ff3366" stopOpacity={0.03} />
                  <stop offset={1} stopColor="#ff3366" stopOpacity={0.25} />
                </linearGradient>
                <linearGradient id="labStroke" x1="0" y1="0" x2="0" y2="1">
                  <stop offset={0} stopColor="#00ff88" />
                  <stop offset={zeroOffset} stopColor="#00ff88" />
                  <stop offset={zeroOffset} stopColor="#ff3366" />
                  <stop offset={1} stopColor="#ff3366" />
                </linearGradient>
              </defs>
              <CartesianGrid strokeDasharray="3 3" stroke="rgba(59, 130, 246, 0.06)" vertical={false} />
              <XAxis dataKey="date" tick={{ fill: "var(--text-secondary)", fontSize: 11 }} axisLine={false} tickLine={false} />
              <YAxis tick={{ fill: "var(--text-secondary)", fontSize: 11 }} tickFormatter={yFormatter} axisLine={false} tickLine={false} width={75} />
              <Tooltip contentStyle={TOOLTIP_STYLE} labelStyle={{ color: "var(--accent-blue)" }} formatter={tooltipFormatter} />
              <ReferenceLine y={refValue} stroke="rgba(100, 116, 139, 0.3)" strokeDasharray="4 4" />
              <Area type="monotone" dataKey={dataKey} stroke="url(#labStroke)" strokeWidth={2} fill="url(#labFill)" animationDuration={800} />
            </AreaChart>
          </ResponsiveContainer>
        )}
      </motion.div>

      {/* Stats Grid */}
      {data && (
        <motion.div
          variants={staggerContainer}
          initial="initial"
          animate="animate"
          className="grid grid-cols-2 sm:grid-cols-3 lg:grid-cols-6 gap-3"
        >
          {[
            {
              label: "Final Value",
              value: formatUsd(String(data.summary.final_value)),
              glow: data.summary.final_value >= initialCap ? "glow-green" : "glow-red",
              border: data.summary.final_value >= initialCap ? "border-[var(--neon-green)]/20 shadow-[0_0_15px_rgba(0,255,136,0.06)]" : "border-[var(--neon-red)]/20 shadow-[0_0_15px_rgba(255,51,102,0.06)]",
            },
            {
              label: "Return",
              value: `${data.summary.total_return_pct >= 0 ? "+" : ""}${data.summary.total_return_pct.toFixed(1)}%`,
              glow: data.summary.total_return_pct >= 0 ? "glow-green" : "glow-red",
              border: data.summary.total_return_pct >= 0 ? "border-[var(--neon-green)]/20 shadow-[0_0_15px_rgba(0,255,136,0.06)]" : "border-[var(--neon-red)]/20 shadow-[0_0_15px_rgba(255,51,102,0.06)]",
            },
            {
              label: "Win Rate",
              value: `${data.summary.win_rate.toFixed(1)}%`,
              glow: data.summary.win_rate >= 50 ? "glow-green" : "glow-red",
              border: data.summary.win_rate >= 50 ? "border-[var(--neon-green)]/20 shadow-[0_0_15px_rgba(0,255,136,0.06)]" : "border-[var(--neon-red)]/20 shadow-[0_0_15px_rgba(255,51,102,0.06)]",
            },
            {
              label: "Max Drawdown",
              value: `${data.summary.max_drawdown_pct.toFixed(1)}%`,
              glow: "glow-red",
              border: "border-[var(--neon-red)]/20 shadow-[0_0_15px_rgba(255,51,102,0.06)]",
            },
            {
              label: "Positions",
              value: formatNumber(data.summary.positions_count),
              glow: "text-[var(--text-primary)]",
              border: "border-[var(--accent-blue)]/15 shadow-[0_0_15px_rgba(59,130,246,0.04)]",
            },
            {
              label: "Traders",
              value: String(data.summary.traders_count),
              glow: "glow-blue",
              border: "border-[var(--accent-blue)]/20 shadow-[0_0_15px_rgba(59,130,246,0.06)]",
            },
          ].map((stat) => (
            <motion.div
              key={stat.label}
              variants={statCardVariants}
              whileHover={hoverGlow}
              className={`glass p-4 text-center border ${stat.border}`}
            >
              <div className="text-[10px] uppercase tracking-widest text-[var(--text-secondary)] mb-2">
                {stat.label}
              </div>
              <div className={`font-mono font-black text-lg ${stat.glow}`}>{stat.value}</div>
            </motion.div>
          ))}
        </motion.div>
      )}

      {/* Trader Breakdown */}
      {data && data.traders.length > 0 && (
        <motion.div
          variants={panelVariants}
          initial="initial"
          animate="animate"
          transition={{ delay: 0.15 }}
          className="glass overflow-hidden gradient-border-top"
        >
          <div className="px-6 py-4">
            <SectionHeader dot="bg-[var(--accent-orange)] shadow-[0_0_6px_var(--accent-orange)]">
              Trader Breakdown
            </SectionHeader>
          </div>
          <div className="overflow-x-auto">
            <table className="w-full text-sm">
              <thead>
                <tr className="border-b border-[var(--border-glow)] text-[var(--text-secondary)] text-xs uppercase tracking-widest">
                  <th className="px-6 py-3 text-left font-medium w-14">#</th>
                  <th className="px-6 py-3 text-left font-medium">Address</th>
                  <th className="px-6 py-3 text-right font-medium">Scaled PnL</th>
                  <th className="px-6 py-3 text-right font-medium">Raw PnL</th>
                  <th className="px-6 py-3 text-right font-medium">Markets</th>
                  <th className="px-6 py-3 text-right font-medium">Scale</th>
                  <th className="px-6 py-3 text-right font-medium">Contribution</th>
                </tr>
              </thead>
              <tbody>
                {data.traders.map((t, i) => {
                  const scaled = parseFloat(t.scaled_pnl);
                  return (
                    <motion.tr
                      key={t.address}
                      variants={tableRowVariants}
                      initial="initial"
                      animate="animate"
                      transition={{ duration: 0.25, delay: i * 0.03 }}
                      className="border-b border-[var(--border-subtle)] row-glow"
                    >
                      <td className={`px-6 py-3 font-mono text-sm ${rankClass(t.rank)}`}>
                        {t.rank}
                      </td>
                      <td className="px-6 py-3">
                        <Link to={`/trader/${t.address}`} className="font-mono text-[var(--accent-blue)] hover:text-white hover:underline transition-colors">
                          {shortenAddress(t.address)}
                        </Link>
                      </td>
                      <td className={`px-6 py-3 text-right font-mono font-bold ${scaled >= 0 ? "glow-green" : "glow-red"}`}>
                        {formatUsd(t.scaled_pnl)}
                      </td>
                      <td className="px-6 py-3 text-right font-mono text-[var(--text-secondary)]">
                        {formatUsd(t.pnl)}
                      </td>
                      <td className="px-6 py-3 text-right font-mono text-[var(--text-secondary)]">
                        {formatNumber(t.markets_traded)}
                      </td>
                      <td className="px-6 py-3 text-right font-mono text-[var(--text-secondary)]">
                        {t.scale_factor.toFixed(3)}x
                      </td>
                      <td className="px-6 py-3 text-right font-mono text-[var(--text-secondary)]">
                        {t.contribution_pct.toFixed(1)}%
                      </td>
                    </motion.tr>
                  );
                })}
              </tbody>
            </table>
          </div>
        </motion.div>
      )}
    </>
  );
}
