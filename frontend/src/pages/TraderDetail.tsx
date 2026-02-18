import { useState } from "react";
import { useParams, Link } from "react-router-dom";
import { useQuery, keepPreviousData } from "@tanstack/react-query";
import { fetchTrader, fetchTraderTrades, fetchTraderPositions, fetchPnlChart } from "../api";
import type { OpenPosition } from "../types";
import Spinner from "../components/Spinner";
import Pagination from "../components/Pagination";
import PnlChart from "../charts/PnlChart";
import { formatUsd, formatNumber, formatDate, formatTimestamp, shortenAddress, polygonscanAddress, polygonscanTx, polymarketAddress } from "../lib/format";

const PAGE_SIZE = 50;
type PosTab = "open" | "closed";

export default function TraderDetail() {
  const { address } = useParams<{ address: string }>();
  const [sideFilter, setSideFilter] = useState("");
  const [offset, setOffset] = useState(0);
  const [posTab, setPosTab] = useState<PosTab>("open");

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
    queryKey: ["pnlChart", address],
    queryFn: () => fetchPnlChart(address!),
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
        <Link to="/" className="text-sm text-[var(--text-secondary)] hover:text-[var(--accent-cyan)] transition-colors duration-200">
          ← Back to Leaderboard
        </Link>
        <div className="flex items-center gap-3 mt-3">
          <h1 className="text-2xl font-black font-mono gradient-text">{shortenAddress(address!)}</h1>
          <button
            onClick={() => navigator.clipboard.writeText(address!)}
            className="text-[var(--text-secondary)] hover:text-[var(--accent-cyan)] transition-colors duration-200"
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
            className="text-[var(--text-secondary)] hover:text-[var(--accent-cyan)] transition-colors duration-200 text-sm"
          >
            Polygonscan ↗
          </a>
          <a
            href={polymarketAddress(address!)}
            target="_blank"
            rel="noopener noreferrer"
            className="text-[var(--text-secondary)] hover:text-[var(--accent-cyan)] transition-colors duration-200 text-sm"
          >
            Polymarket ↗
          </a>
        </div>
      </div>

      {/* Stats Cards */}
      <div className="grid grid-cols-2 md:grid-cols-3 gap-4">
        <StatCard label="PnL" value={formatUsd(trader.realized_pnl)} glow={pnl >= 0 ? "green" : "red"} />
        <StatCard label="Total Volume" value={formatUsd(trader.total_volume)} />
        <StatCard label="Trades" value={formatNumber(trader.trade_count)} />
        <StatCard label="Markets" value={formatNumber(trader.markets_traded)} />
        <StatCard label="Total Fees" value={formatUsd(trader.total_fees)} />
        <StatCard label="Active Since" value={formatDate(trader.first_trade)} />
      </div>

      {/* Cumulative PnL Chart */}
      {pnlChart && pnlChart.points.length > 0 && <PnlChart points={pnlChart.points} />}

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
                <button
                  key={tab.value}
                  onClick={() => setPosTab(tab.value)}
                  className={`px-4 py-1.5 text-xs rounded-full font-medium transition-all duration-200 ${
                    posTab === tab.value
                      ? "bg-[var(--accent-cyan)]/10 text-[var(--accent-cyan)] border border-[var(--accent-cyan)]/30 shadow-[0_0_8px_rgba(34,211,238,0.15)]"
                      : "text-[var(--text-secondary)] border border-transparent hover:text-[var(--text-primary)] hover:border-[var(--border-glow)]"
                  }`}
                >
                  {tab.label} ({tab.count})
                </button>
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
              <button
                key={s}
                onClick={() => { setSideFilter(s); setOffset(0); }}
                className={`px-4 py-1.5 text-xs rounded-full font-medium transition-all duration-200 ${
                  sideFilter === s
                    ? "bg-[var(--accent-cyan)]/10 text-[var(--accent-cyan)] border border-[var(--accent-cyan)]/30 shadow-[0_0_8px_rgba(34,211,238,0.15)]"
                    : "text-[var(--text-secondary)] border border-transparent hover:text-[var(--text-primary)] hover:border-[var(--border-glow)]"
                }`}
              >
                {s === "" ? "All" : s === "buy" ? "Buy" : "Sell"}
              </button>
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
                      <tr key={`${t.tx_hash}-${i}`} className="border-b border-[var(--border-subtle)] row-glow">
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
                            className="text-[var(--accent-cyan)]/50 hover:text-[var(--accent-cyan)] font-mono text-xs transition-colors duration-200"
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
            {positions.map((p) => {
              const positionPnl = parseFloat(p.pnl);
              return (
                <tr key={p.asset_id} className="border-b border-[var(--border-subtle)] row-glow">
                  <td className="px-4 py-3 text-[var(--text-primary)] max-w-xs truncate" title={p.question}>
                    {p.question}
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
                </tr>
              );
            })}
          </tbody>
        </table>
      </div>
    </div>
  );
}

function StatCard({ label, value, glow }: { label: string; value: string; glow?: "green" | "red" }) {
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
    <div className={`glass p-4 gradient-border-top ${borderColor}`}>
      <div className="text-xs text-[var(--text-secondary)] mb-2 uppercase tracking-wider">{label}</div>
      <div className={`text-xl font-bold font-mono ${valueClass}`}>{value}</div>
    </div>
  );
}
