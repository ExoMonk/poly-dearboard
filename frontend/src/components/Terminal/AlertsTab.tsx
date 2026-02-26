import { useState, useRef, useEffect } from "react";
import { Link, useNavigate } from "react-router-dom";
import type { Alert } from "../../types";
import { formatUsd, shortenAddress, timeAgo, polygonscanTx } from "../../lib/format";
import EventActions, { type ActionDef } from "../EventActions";
import AddToListButton from "../AddToListButton";
import { requestOpenCreateSession } from "./CreateSessionModal";

type Filter = "all" | "WhaleTrade" | "MarketResolution" | "FailedSettlement";
const FILTERS: { label: string; value: Filter }[] = [
  { label: "All", value: "all" },
  { label: "Whales", value: "WhaleTrade" },
  { label: "Resolved", value: "MarketResolution" },
  { label: "Failed", value: "FailedSettlement" },
];

interface Props {
  alerts: Alert[];
  connected: boolean;
}

export function AlertsTab({ alerts, connected }: Props) {
  const [filter, setFilter] = useState<Filter>("all");
  const listRef = useRef<HTMLDivElement>(null);
  const navigate = useNavigate();

  // Track new alerts for flash highlight
  const seenRef = useRef<Set<string>>(new Set());
  const [flashIds, setFlashIds] = useState<Set<string>>(new Set());
  useEffect(() => {
    const newIds: string[] = [];
    for (const a of alerts) {
      const key = `${a.tx_hash}:${a.kind}`;
      if (!seenRef.current.has(key)) {
        seenRef.current.add(key);
        newIds.push(key);
      }
    }
    if (newIds.length > 0) {
      setFlashIds((prev) => new Set([...prev, ...newIds]));
      const t = setTimeout(() => setFlashIds((prev) => {
        const next = new Set(prev);
        for (const id of newIds) next.delete(id);
        return next;
      }), 2000);
      return () => clearTimeout(t);
    }
  }, [alerts]);

  const filtered = filter === "all" ? alerts : alerts.filter((a) => a.kind === filter);

  return (
    <div className="flex flex-col h-full">
      {/* Header */}
      <div className="flex items-center gap-2 px-3 py-1.5 border-b border-white/5 shrink-0">
        <span className={`w-1.5 h-1.5 rounded-full ${connected ? "bg-green-500" : "bg-red-500"}`} />
        <span className="text-[10px] text-[var(--text-muted)]">{connected ? "Live" : "Reconnecting"}</span>
        <div className="flex gap-1 ml-auto">
          {FILTERS.map((f) => (
            <button
              key={f.value}
              onClick={() => setFilter(f.value)}
              className={`text-[10px] px-1.5 py-0.5 rounded transition-colors ${
                filter === f.value
                  ? "bg-[var(--accent-blue)]/20 text-[var(--accent-blue)]"
                  : "text-[var(--text-muted)] hover:text-[var(--text-secondary)]"
              }`}
            >
              {f.label}
            </button>
          ))}
        </div>
      </div>

      {/* List */}
      <div ref={listRef} className="flex-1 min-h-0 overflow-y-auto">
        {filtered.length === 0 && (
          <div className="flex items-center justify-center h-full text-xs text-[var(--text-muted)]">
            {connected ? "Waiting for alerts..." : "Connecting..."}
          </div>
        )}
        {filtered.map((alert) => {
          const key = `${alert.tx_hash}:${alert.kind}`;
          const flashing = flashIds.has(key);
          return (
            <div
              key={key}
              className={`flex items-center gap-2 px-3 py-2 border-b border-white/[0.03] text-xs hover:bg-white/[0.02] transition-colors ${
                flashing ? "delta-flash-neutral" : ""
              }`}
            >
              <AlertRow alert={alert} navigate={navigate} />
            </div>
          );
        })}
      </div>
    </div>
  );
}

function AlertRow({ alert, navigate }: { alert: Alert; navigate: ReturnType<typeof useNavigate> }) {
  switch (alert.kind) {
    case "WhaleTrade": {
      const isBuy = alert.side?.toLowerCase() === "buy";
      const actions: ActionDef[] = [
        { kind: "add_trader" as const, onClick: () => {}, render: <AddToListButton address={alert.trader} /> },
        { kind: "copy_trade" as const, onClick: () => requestOpenCreateSession({ sourceSurface: "alerts", defaults: { simulationMode: "simulate" }, question: alert.question }) },
        { kind: "open_trader" as const, onClick: () => navigate(`/trader/${alert.trader}`) },
        { kind: "open_market" as const, onClick: () => navigate(`/market/${encodeURIComponent(alert.asset_id)}`) },
      ];
      return (
        <>
          <span className={`text-[10px] font-bold px-1.5 py-0.5 rounded-full shrink-0 ${
            isBuy ? "text-[var(--neon-green)] bg-[var(--neon-green)]/10" : "text-[var(--neon-red)] bg-[var(--neon-red)]/10"
          }`}>
            {isBuy ? "BUY" : "SELL"}
          </span>
          <Link to={`/trader/${alert.trader}`} className="font-mono text-[var(--accent-blue)] hover:text-white shrink-0 transition-colors">
            {shortenAddress(alert.trader)}
          </Link>
          <span className="text-[var(--text-primary)] font-medium shrink-0">{formatUsd(alert.usdc_amount)}</span>
          <span className="text-[var(--text-muted)] truncate flex-1" title={alert.question}>{alert.question ?? alert.asset_id.slice(0, 16)}</span>
          <span className="text-[var(--text-muted)] shrink-0">{timeAgo(alert.timestamp)}</span>
          <EventActions actions={actions} />
        </>
      );
    }
    case "MarketResolution": {
      const actions: ActionDef[] = [
        { kind: "open_market" as const, onClick: () => { if (alert.token_id) navigate(`/market/${encodeURIComponent(alert.token_id)}`); } },
      ];
      return (
        <>
          <span className="text-[10px] font-bold px-1.5 py-0.5 rounded-full bg-[var(--accent-blue)]/10 text-[var(--accent-blue)] shrink-0">RESOLVED</span>
          <span className="text-[var(--text-primary)] font-medium shrink-0">{alert.winning_outcome ?? "—"}</span>
          <span className="text-[var(--text-muted)] truncate flex-1" title={alert.question}>{alert.question ?? alert.condition_id.slice(0, 16)}</span>
          <span className="text-[var(--text-muted)] shrink-0">{timeAgo(alert.timestamp)}</span>
          <EventActions actions={actions} />
        </>
      );
    }
    case "FailedSettlement": {
      const actions: ActionDef[] = [
        { kind: "open_tx" as const, onClick: () => window.open(polygonscanTx(alert.tx_hash), "_blank") },
        { kind: "open_trader" as const, onClick: () => navigate(`/trader/${alert.from_address}`) },
      ];
      return (
        <>
          <span className="text-[10px] font-bold px-1.5 py-0.5 rounded-full bg-red-500/10 text-red-400 shrink-0 animate-pulse">FAILED</span>
          <span className="text-[var(--text-primary)] font-mono shrink-0">{alert.function_name}</span>
          <Link to={`/trader/${alert.from_address}`} className="font-mono text-[var(--accent-blue)] hover:text-white shrink-0 transition-colors">
            {shortenAddress(alert.from_address)}
          </Link>
          <span className="text-[var(--text-muted)] flex-1" />
          <span className="text-[var(--text-muted)] shrink-0">{timeAgo(alert.timestamp)}</span>
          <EventActions actions={actions} />
        </>
      );
    }
  }
}
