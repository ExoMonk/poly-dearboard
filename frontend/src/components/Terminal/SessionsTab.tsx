import { useState } from "react";
import { useSessions, useUpdateSession, useDeleteSession, useSessionStats } from "../../hooks/useCopyTrade";
import { useTerminalDispatch } from "./TerminalProvider";
import { PositionList } from "./PositionList";
import { requestOpenCreateSession } from "./CreateSessionModal";
import type { CopyTradeSession, SessionStatus } from "../../types";

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
            {session.simulate ? (
              <span className="text-xs font-mono px-1.5 py-0.5 rounded border bg-blue-500/20 text-blue-400 border-blue-500/30">
                SIM
              </span>
            ) : (
              <span className="text-xs font-mono px-1.5 py-0.5 rounded border bg-red-500/20 text-red-400 border-red-500/30">
                LIVE
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

        {!session.simulate && session.wallet_id && (
          <div className="text-xs mb-2">
            <div className="text-[var(--text-muted)]">Wallet</div>
            <div className="font-mono">{session.wallet_id.slice(0, 8)}</div>
          </div>
        )}

        {/* Exit strategy chips + utilization */}
        <div className="flex flex-wrap gap-1 text-[10px] mb-1">
          {session.take_profit_pct != null && (
            <span className="px-1.5 py-0.5 rounded bg-green-500/10 text-green-400 border border-green-500/20">
              TP: {session.take_profit_pct}%
            </span>
          )}
          {session.stop_loss_pct != null && (
            <span className="px-1.5 py-0.5 rounded bg-red-500/10 text-red-400 border border-red-500/20">
              SL: {session.stop_loss_pct}%
            </span>
          )}
          {session.mirror_close && (
            <span className="px-1.5 py-0.5 rounded bg-blue-500/10 text-blue-400 border border-blue-500/20">
              Mirror
            </span>
          )}
          {(session.max_source_price < 0.95 || session.min_source_price > 0.05) && (
            <span className="px-1.5 py-0.5 rounded bg-purple-500/10 text-purple-400 border border-purple-500/20">
              {(session.min_source_price * 100).toFixed(0)}¢–{(session.max_source_price * 100).toFixed(0)}¢
            </span>
          )}
          {stats && session.utilization_cap < 1.0 && stats.capital_utilization >= session.utilization_cap && (
            <span className="px-1.5 py-0.5 rounded bg-orange-500/10 text-orange-400 border border-orange-500/20">
              Buy-gated
            </span>
          )}
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

export function SessionsTab() {
  const { data: sessions, isLoading } = useSessions();

  if (isLoading) {
    return (
      <div className="flex items-center justify-center h-full text-xs text-[var(--text-secondary)]">
        Loading sessions...
      </div>
    );
  }

  if (!sessions || sessions.length === 0) {
    return (
      <div className="flex flex-col items-center justify-center h-full gap-3">
        <div className="text-xs text-[var(--text-secondary)]">No sessions yet</div>
        <button
          className="px-4 py-2 text-xs font-semibold rounded-xl bg-[var(--accent-blue)]/15 text-[var(--accent-blue)] border border-[var(--accent-blue)]/20 hover:bg-[var(--accent-blue)]/25 transition-colors"
          onClick={requestOpenCreateSession}
        >
          New Session
        </button>
      </div>
    );
  }

  return (
    <div className="p-3 space-y-2 overflow-y-auto h-full">
      <button
        className="w-full py-1.5 text-xs font-semibold rounded-xl bg-[var(--accent-blue)]/15 text-[var(--accent-blue)] border border-[var(--accent-blue)]/20 hover:bg-[var(--accent-blue)]/25 transition-colors mb-1"
        onClick={requestOpenCreateSession}
      >
        New Session
      </button>
      {sessions.map((s) => (
        <SessionCard key={s.id} session={s} />
      ))}
    </div>
  );
}
