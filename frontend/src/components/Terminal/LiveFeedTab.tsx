import { useState, useRef, useEffect, useMemo } from "react";
import { Link, useNavigate } from "react-router-dom";
import type { FeedTrade, SignalTrade, ConvergenceAlert, LiveFeedMode } from "../../types";
import { formatUsd, shortenAddress, timeAgo } from "../../lib/format";
import EventActions, { type ActionDef } from "../EventActions";
import AddToListButton from "../AddToListButton";
import { requestOpenCreateSession } from "./CreateSessionModal";
import { useTraderLists } from "../../hooks/useTraderLists";

const MAX_DISPLAY = 100;

interface Props {
  mode: LiveFeedMode;
  onSetMode: (mode: LiveFeedMode) => void;
  listId: string | null;
  onSetListId: (id: string | null) => void;
  connected: boolean;
  isLagging?: boolean;
  // Signal mode data
  signalTrades?: SignalTrade[];
  convergenceAlerts?: ConvergenceAlert[];
  // Public mode data
  publicTrades?: FeedTrade[];
}

export function LiveFeedTab({ mode, onSetMode, listId, onSetListId, connected, isLagging, signalTrades, convergenceAlerts, publicTrades }: Props) {
  const navigate = useNavigate();
  const listRef = useRef<HTMLDivElement>(null);
  const { data: lists } = useTraderLists();

  // Flash highlight for new items
  const seenRef = useRef<Set<string>>(new Set());
  const [flashIds, setFlashIds] = useState<Set<string>>(new Set());

  const trades = useMemo<(SignalTrade | FeedTrade)[]>(
    () => mode === "signals"
      ? (signalTrades ?? []).slice(0, MAX_DISPLAY)
      : (publicTrades ?? []).slice(0, MAX_DISPLAY),
    [mode, signalTrades, publicTrades],
  );

  useEffect(() => {
    const newIds: string[] = [];
    for (const t of trades) {
      if (!seenRef.current.has(t.tx_hash)) {
        seenRef.current.add(t.tx_hash);
        newIds.push(t.tx_hash);
      }
    }
    if (newIds.length > 0) {
      setFlashIds((prev) => new Set([...prev, ...newIds]));
      const timer = setTimeout(() => setFlashIds((prev) => {
        const next = new Set(prev);
        for (const id of newIds) next.delete(id);
        return next;
      }), 2000);
      return () => clearTimeout(timer);
    }
  }, [trades]);

  return (
    <div className="flex flex-col h-full">
      {/* Header */}
      <div className="flex items-center gap-2 px-3 py-1.5 border-b border-white/5 shrink-0">
        <span className={`w-1.5 h-1.5 rounded-full ${connected ? "bg-green-500" : "bg-red-500"}`} />
        <span className="text-[10px] text-[var(--text-muted)]">{connected ? "Live" : "Connecting"}</span>
        {isLagging && <span className="text-[10px] text-[var(--accent-orange)]">lag</span>}
        {mode === "signals" && (
          <select
            value={listId ?? ""}
            onChange={(e) => onSetListId(e.target.value || null)}
            className="text-[10px] px-1 py-0.5 rounded bg-[var(--bg-deep)] border border-white/10 text-[var(--text-secondary)] outline-none cursor-pointer max-w-[140px] truncate"
            title="Signal source"
          >
            <option value="">Top 20</option>
            {(lists ?? []).map((l) => (
              <option key={l.id} value={l.id}>{l.name} ({l.member_count})</option>
            ))}
          </select>
        )}
        <div className="flex gap-1 ml-auto">
          <button
            onClick={() => onSetMode("signals")}
            className={`text-[10px] px-1.5 py-0.5 rounded transition-colors ${
              mode === "signals" ? "bg-[var(--accent-blue)]/20 text-[var(--accent-blue)]" : "text-[var(--text-muted)] hover:text-[var(--text-secondary)]"
            }`}
          >
            Signals
          </button>
          <button
            onClick={() => onSetMode("public")}
            className={`text-[10px] px-1.5 py-0.5 rounded transition-colors ${
              mode === "public" ? "bg-[var(--accent-blue)]/20 text-[var(--accent-blue)]" : "text-[var(--text-muted)] hover:text-[var(--text-secondary)]"
            }`}
          >
            Public
          </button>
        </div>
      </div>

      {/* Convergence alerts (signals mode only) */}
      {mode === "signals" && convergenceAlerts && convergenceAlerts.length > 0 && (
        <div className="px-3 py-1.5 border-b border-[var(--accent-orange)]/20 bg-[var(--accent-orange)]/[0.03]">
          {convergenceAlerts.slice(0, 3).map((a, i) => (
            <div key={`${a.asset_id}-${i}`} className="flex items-center gap-2 text-[10px] py-0.5">
              <span className="font-bold text-[var(--accent-orange)] uppercase">Convergence</span>
              <span className={`font-bold ${
                a.side.toLowerCase() === "buy" ? "text-[var(--neon-green)]" : "text-[var(--neon-red)]"
              }`}>{a.side}</span>
              <span className="text-[var(--text-muted)] truncate">{a.question ?? a.asset_id.slice(0, 16)}</span>
              <span className="text-[var(--text-muted)] shrink-0">{a.traders.length} traders</span>
            </div>
          ))}
        </div>
      )}

      {/* Trade list */}
      <div ref={listRef} className="flex-1 min-h-0 overflow-y-auto">
        {trades.length === 0 && (
          <div className="flex items-center justify-center h-full text-xs text-[var(--text-muted)]">
            {connected ? "Waiting for trades..." : "Connecting..."}
          </div>
        )}
        {trades.map((trade) => {
          const flashing = flashIds.has(trade.tx_hash);
          const isBuy = trade.side?.toLowerCase() === "buy";
          const showAddTrader = !(mode === "signals" && listId);
          const actions: ActionDef[] = [
            ...(showAddTrader ? [{ kind: "add_trader" as const, onClick: () => {}, render: <AddToListButton address={trade.trader} /> }] : []),
            { kind: "copy_trade" as const, onClick: () => requestOpenCreateSession({ sourceSurface: "feed", defaults: { simulationMode: "simulate" }, question: trade.question }) },
            { kind: "open_trader" as const, onClick: () => navigate(`/trader/${trade.trader}`) },
          ];
          return (
            <div
              key={trade.tx_hash}
              className={`flex items-center gap-2 px-3 py-2 border-b border-white/[0.03] text-xs hover:bg-white/[0.02] transition-colors ${
                flashing ? "delta-flash-neutral" : ""
              }`}
            >
              <span className={`text-[10px] font-bold px-1.5 py-0.5 rounded-full shrink-0 ${
                isBuy ? "text-[var(--neon-green)] bg-[var(--neon-green)]/10" : "text-[var(--neon-red)] bg-[var(--neon-red)]/10"
              }`}>
                {isBuy ? "BUY" : "SELL"}
              </span>
              <Link to={`/trader/${trade.trader}`} className="font-mono text-[var(--accent-blue)] hover:text-white shrink-0 transition-colors">
                {shortenAddress(trade.trader)}
              </Link>
              <span className="text-[var(--text-primary)] font-medium shrink-0">{formatUsd(trade.usdc_amount)}</span>
              <span className="text-[var(--text-muted)] truncate flex-1" title={trade.question}>
                {trade.question ?? trade.asset_id.slice(0, 16)}
                {trade.outcome && <span className="text-[var(--accent-orange)] ml-1">({trade.outcome})</span>}
              </span>
              <span className="text-[var(--text-muted)] shrink-0">{timeAgo(trade.block_timestamp)}</span>
              <EventActions actions={actions} />
            </div>
          );
        })}
      </div>
    </div>
  );
}
