import { useState } from "react";
import { useSessions, useUpdateSession, useDeleteSession, useCreateSession, useSessionStats } from "../../hooks/useCopyTrade";
import { useTraderLists } from "../../hooks/useTraderLists";
import { useTerminalDispatch } from "./TerminalProvider";
import { PositionList } from "./PositionList";
import type { CopyTradeSession, SessionStatus, CopyOrderType, CreateSessionRequest } from "../../types";

const STATUS_BADGE: Record<SessionStatus, string> = {
  running: "bg-green-500/20 text-green-400 border-green-500/30",
  paused: "bg-yellow-500/20 text-yellow-400 border-yellow-500/30",
  stopped: "bg-neutral-500/20 text-neutral-400 border-neutral-500/30",
};

function formatRuntime(seconds: number): string {
  if (seconds < 60) return `${seconds}s`;
  if (seconds < 3600) return `${Math.floor(seconds / 60)}m`;
  const h = Math.floor(seconds / 3600);
  const m = Math.floor((seconds % 3600) / 60);
  return `${h}h ${m}m`;
}

function SessionCard({ session }: { session: CopyTradeSession }) {
  const update = useUpdateSession();
  const del = useDeleteSession();
  const { setActiveTab } = useTerminalDispatch();
  const [expanded, setExpanded] = useState(false);
  const { data: stats } = useSessionStats(session.id);

  // Prefer live CLOB-based P&L from stats when available, fall back to stale session values
  const staleValue = session.remaining_capital + session.positions_value;
  const stalePnl = staleValue - session.initial_capital;
  const totalValue = stats ? session.initial_capital + stats.total_pnl : staleValue;
  const pnl = stats ? stats.total_pnl : stalePnl;
  const pnlPct = stats ? stats.return_pct : (stalePnl / session.initial_capital) * 100;

  return (
    <div className="rounded border border-[var(--border-subtle)] bg-[var(--surface-1)]">
      <div
        className="p-3 cursor-pointer"
        onClick={() => setExpanded(!expanded)}
      >
        <div className="flex items-center justify-between mb-2">
          <div className="flex items-center gap-2">
            <span className={`text-xs font-mono px-1.5 py-0.5 rounded border ${STATUS_BADGE[session.status]}`}>
              {session.status}
            </span>
            {session.simulate && (
              <span className="text-xs font-mono px-1.5 py-0.5 rounded border bg-blue-500/20 text-blue-400 border-blue-500/30">
                SIM
              </span>
            )}
            <span className="text-[10px] text-[var(--text-muted)]">{expanded ? "\u25B2" : "\u25BC"}</span>
          </div>
          <span className="text-xs text-[var(--text-muted)] font-mono">{session.id.slice(0, 8)}</span>
        </div>

        <div className="grid grid-cols-3 gap-2 text-xs mb-2">
          <div>
            <div className="text-[var(--text-muted)]">Value</div>
            <div className="font-mono">${totalValue.toFixed(2)}</div>
          </div>
          <div>
            <div className="text-[var(--text-muted)]">P&L</div>
            <div className={`font-mono ${pnl >= 0 ? "text-green-400" : "text-red-400"}`}>
              {pnl >= 0 ? "+" : ""}{pnl.toFixed(2)} ({pnlPct >= 0 ? "+" : ""}{pnlPct.toFixed(1)}%)
            </div>
          </div>
          <div>
            <div className="text-[var(--text-muted)]">Type</div>
            <div className="font-mono">{session.order_type} @ {(session.copy_pct * 100).toFixed(0)}%</div>
          </div>
        </div>

        <div className="grid grid-cols-2 gap-2 text-xs mb-2">
          <div>
            <div className="text-[var(--text-muted)]">Source</div>
            <div className="font-mono">{session.list_id ? `List` : `Top ${session.top_n}`}</div>
          </div>
          <div>
            <div className="text-[var(--text-muted)]">Slippage</div>
            <div className="font-mono">{session.max_slippage_bps}bps max</div>
          </div>
        </div>
      </div>

      {/* Expanded stats + positions */}
      {expanded && stats && (
        <div className="px-3 pb-2 border-t border-[var(--border-subtle)]">
          <div className="grid grid-cols-4 gap-2 text-xs py-2">
            <div>
              <div className="text-[var(--text-muted)]">Orders</div>
              <div className="font-mono">{stats.filled_orders}/{stats.total_orders}</div>
            </div>
            <div>
              <div className="text-[var(--text-muted)]">Win Rate</div>
              <div className="font-mono">{stats.win_rate.toFixed(0)}%</div>
            </div>
            <div>
              <div className="text-[var(--text-muted)]">Avg Slip</div>
              <div className="font-mono">{stats.avg_slippage_bps.toFixed(1)}bps</div>
            </div>
            <div>
              <div className="text-[var(--text-muted)]">Runtime</div>
              <div className="font-mono">{formatRuntime(stats.runtime_seconds)}</div>
            </div>
          </div>
          <div className="grid grid-cols-3 gap-2 text-xs pb-2">
            <div>
              <div className="text-[var(--text-muted)]">Realized</div>
              <div className={`font-mono ${stats.realized_pnl >= 0 ? "text-green-400" : "text-red-400"}`}>
                {stats.realized_pnl >= 0 ? "+" : ""}{stats.realized_pnl.toFixed(2)}
              </div>
            </div>
            <div>
              <div className="text-[var(--text-muted)]">Unrealized</div>
              <div className={`font-mono ${stats.unrealized_pnl >= 0 ? "text-green-400" : "text-red-400"}`}>
                {stats.unrealized_pnl >= 0 ? "+" : ""}{stats.unrealized_pnl.toFixed(2)}
              </div>
            </div>
            <div>
              <div className="text-[var(--text-muted)]">Utilization</div>
              <div className="font-mono">{(stats.capital_utilization * 100).toFixed(0)}%</div>
            </div>
          </div>

          <PositionList sessionId={session.id} canClose={session.status !== "stopped"} />
        </div>
      )}

      {/* Action buttons */}
      <div className="flex gap-1.5 px-3 pb-3">
        {session.status === "running" && (
          <button
            className="px-2 py-0.5 text-xs rounded bg-yellow-500/20 text-yellow-400 hover:bg-yellow-500/30"
            onClick={() => update.mutate({ id: session.id, action: "pause" })}
            disabled={update.isPending}
          >
            Pause
          </button>
        )}
        {session.status === "paused" && (
          <button
            className="px-2 py-0.5 text-xs rounded bg-green-500/20 text-green-400 hover:bg-green-500/30"
            onClick={() => update.mutate({ id: session.id, action: "resume" })}
            disabled={update.isPending}
          >
            Resume
          </button>
        )}
        {session.status !== "stopped" && (
          <button
            className="px-2 py-0.5 text-xs rounded bg-red-500/20 text-red-400 hover:bg-red-500/30"
            onClick={() => update.mutate({ id: session.id, action: "stop" })}
            disabled={update.isPending}
          >
            Stop
          </button>
        )}
        {session.status === "stopped" && (
          <button
            className="px-2 py-0.5 text-xs rounded bg-neutral-500/20 text-neutral-400 hover:bg-neutral-500/30"
            onClick={() => del.mutate(session.id)}
            disabled={del.isPending}
          >
            Delete
          </button>
        )}
        <button
          className="px-2 py-0.5 text-xs rounded bg-[var(--surface-2)] text-[var(--text-muted)] hover:text-[var(--text-primary)] ml-auto"
          onClick={() => setActiveTab("orders")}
        >
          View Orders
        </button>
      </div>
    </div>
  );
}

function CreateSessionForm({ onClose }: { onClose: () => void }) {
  const create = useCreateSession();
  const { data: lists } = useTraderLists();

  const [source, setSource] = useState<"top_n" | "list">("top_n");
  const [topN, setTopN] = useState(10);
  const [listId, setListId] = useState("");
  const [capital, setCapital] = useState(1000);
  const [copyPct, setCopyPct] = useState(50);
  const [maxPosition, setMaxPosition] = useState(100);
  const [maxSlippage, setMaxSlippage] = useState(200);
  const [orderType, setOrderType] = useState<CopyOrderType>("FOK");
  const [maxLossPct, setMaxLossPct] = useState(20);
  const [error, setError] = useState("");

  const handleSubmit = () => {
    setError("");
    const body: CreateSessionRequest = {
      copy_pct: copyPct / 100,
      max_position_usdc: maxPosition,
      max_slippage_bps: maxSlippage,
      order_type: orderType,
      initial_capital: capital,
      simulate: true,
      max_loss_pct: maxLossPct,
      ...(source === "top_n" ? { top_n: topN } : { list_id: listId }),
    };
    create.mutate(body, {
      onSuccess: () => onClose(),
      onError: (e) => setError(e.message),
    });
  };

  const inputCls = "w-full bg-[var(--surface-2)] border border-[var(--border-subtle)] rounded px-2 py-1 text-xs font-mono text-[var(--text-primary)] focus:outline-none focus:border-[var(--neon-green)]/50";
  const labelCls = "text-[10px] text-[var(--text-muted)] uppercase tracking-wider mb-0.5";

  return (
    <div className="p-3 space-y-3 overflow-y-auto h-full">
      <div className="flex items-center justify-between">
        <span className="text-xs font-semibold text-[var(--text-primary)]">Start Simulation</span>
        <button onClick={onClose} className="text-xs text-[var(--text-muted)] hover:text-[var(--text-primary)]">Cancel</button>
      </div>

      {/* Source */}
      <div>
        <div className={labelCls}>Trader Source</div>
        <div className="flex gap-1.5">
          <button
            className={`px-2 py-1 text-xs rounded border ${source === "top_n" ? "bg-[var(--neon-green)]/10 text-[var(--neon-green)] border-[var(--neon-green)]/30" : "bg-[var(--surface-2)] text-[var(--text-muted)] border-[var(--border-subtle)]"}`}
            onClick={() => setSource("top_n")}
          >
            Top N
          </button>
          <button
            className={`px-2 py-1 text-xs rounded border ${source === "list" ? "bg-[var(--neon-green)]/10 text-[var(--neon-green)] border-[var(--neon-green)]/30" : "bg-[var(--surface-2)] text-[var(--text-muted)] border-[var(--border-subtle)]"}`}
            onClick={() => setSource("list")}
          >
            List
          </button>
        </div>
      </div>

      {source === "top_n" ? (
        <div>
          <div className={labelCls}>Top N traders</div>
          <input type="number" className={inputCls} value={topN} min={1} max={50} onChange={(e) => setTopN(Number(e.target.value))} />
        </div>
      ) : (
        <div>
          <div className={labelCls}>Select List</div>
          <select className={inputCls} value={listId} onChange={(e) => setListId(e.target.value)}>
            <option value="">Choose...</option>
            {lists?.map((l) => (
              <option key={l.id} value={l.id}>{l.name} ({l.member_count})</option>
            ))}
          </select>
        </div>
      )}

      <div className="grid grid-cols-2 gap-2">
        <div>
          <div className={labelCls}>Capital (USDC)</div>
          <input type="number" className={inputCls} value={capital} min={10} onChange={(e) => setCapital(Number(e.target.value))} />
        </div>
        <div>
          <div className={labelCls}>Copy %</div>
          <input type="number" className={inputCls} value={copyPct} min={5} max={100} onChange={(e) => setCopyPct(Number(e.target.value))} />
        </div>
      </div>

      <div className="grid grid-cols-2 gap-2">
        <div>
          <div className={labelCls}>Max Position (USDC)</div>
          <input type="number" className={inputCls} value={maxPosition} min={1} onChange={(e) => setMaxPosition(Number(e.target.value))} />
        </div>
        <div>
          <div className={labelCls}>Max Slippage (bps)</div>
          <input type="number" className={inputCls} value={maxSlippage} min={10} max={1000} onChange={(e) => setMaxSlippage(Number(e.target.value))} />
        </div>
      </div>

      <div className="grid grid-cols-2 gap-2">
        <div>
          <div className={labelCls}>Order Type</div>
          <select className={inputCls} value={orderType} onChange={(e) => setOrderType(e.target.value as CopyOrderType)}>
            <option value="FOK">FOK (Fill or Kill)</option>
            <option value="GTC">GTC (Good til Canceled)</option>
          </select>
        </div>
        <div>
          <div className={labelCls}>Max Loss %</div>
          <input type="number" className={inputCls} value={maxLossPct} min={1} max={100} onChange={(e) => setMaxLossPct(Number(e.target.value))} />
        </div>
      </div>

      {error && <div className="text-xs text-red-400">{error}</div>}

      <button
        className="w-full py-1.5 text-xs font-semibold rounded bg-blue-500/20 text-blue-400 border border-blue-500/30 hover:bg-blue-500/30 disabled:opacity-50"
        onClick={handleSubmit}
        disabled={create.isPending || (source === "list" && !listId)}
      >
        {create.isPending ? "Starting..." : "Start Simulation"}
      </button>
    </div>
  );
}

export function SessionsTab() {
  const { data: sessions, isLoading } = useSessions();
  const [showCreate, setShowCreate] = useState(false);
  if (isLoading) {
    return (
      <div className="flex items-center justify-center h-full text-xs text-[var(--text-muted)]">
        Loading sessions...
      </div>
    );
  }

  if (showCreate) {
    return <CreateSessionForm onClose={() => setShowCreate(false)} />;
  }

  if (!sessions || sessions.length === 0) {
    return (
      <div className="flex flex-col items-center justify-center h-full gap-3">
        <div className="text-xs text-[var(--text-muted)]">No sessions yet</div>
        <button
          className="px-4 py-2 text-xs font-semibold rounded bg-blue-500/20 text-blue-400 border border-blue-500/30 hover:bg-blue-500/30"
          onClick={() => setShowCreate(true)}
        >
          Start Simulation
        </button>
      </div>
    );
  }

  return (
    <div className="p-3 space-y-2 overflow-y-auto h-full">
      <button
        className="w-full py-1.5 text-xs font-semibold rounded bg-blue-500/20 text-blue-400 border border-blue-500/30 hover:bg-blue-500/30 mb-1"
        onClick={() => setShowCreate(true)}
      >
        + New Simulation
      </button>
      {sessions.map((s) => (
        <SessionCard key={s.id} session={s} />
      ))}
    </div>
  );
}
