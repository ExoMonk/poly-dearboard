import { useMemo } from "react";
import { useTerminalState } from "./TerminalProvider";
import { useCopyTradeSummary } from "../../hooks/useCopyTrade";
import { requestOpenCommandPalette } from "./CommandPalette";

interface TerminalStatusBarProps {
  wsConnected: boolean;
  alertsConnected: boolean;
}

function formatRelativeTime(ts: number): string {
  const diff = Math.floor((Date.now() - ts) / 1000);
  if (diff < 60) return `${diff}s ago`;
  if (diff < 3600) return `${Math.floor(diff / 60)}m ago`;
  return `${Math.floor(diff / 3600)}h ago`;
}

function CopyTradeSummaryBar({ activeSessions, totalPnl }: { activeSessions: number; totalPnl: number }) {
  if (activeSessions === 0) return null;

  return (
    <span className="flex items-center gap-1.5">
      <span className="font-mono">{activeSessions} session{activeSessions !== 1 ? "s" : ""}</span>
      <span className={`font-mono ${totalPnl >= 0 ? "text-green-400" : "text-red-400"}`}>
        {totalPnl >= 0 ? "+" : ""}{totalPnl.toFixed(2)}
      </span>
    </span>
  );
}

export function TerminalStatusBar({ wsConnected, alertsConnected }: TerminalStatusBarProps) {
  const { logs, walletBalance } = useTerminalState();
  const { data: summary } = useCopyTradeSummary();
  const isMac = /Mac|iPhone|iPad/.test(navigator.userAgent);
  const commandHint = isMac ? "⌘K" : "Ctrl+K";

  const lastOrderTime = useMemo(() => {
    for (let i = logs.length - 1; i >= 0; i--) {
      if (logs[i].level === "success" && logs[i].meta?.order_id) {
        return logs[i].timestamp;
      }
    }
    return null;
  }, [logs]);

  return (
    <div className="flex items-center h-6 px-3 gap-4 border-t border-white/5 text-[10px] text-[var(--text-muted)]">
      {!!summary?.active_sessions && (
        <span className="flex items-center gap-1.5" title={`Copy-trade WebSocket: ${wsConnected ? "connected" : "disconnected"}`}>
          <span className={`w-1.5 h-1.5 rounded-full ${wsConnected ? "bg-green-500" : "bg-red-500"}`} />
          WS
        </span>
      )}

      <span className="flex items-center gap-1.5" title={`Alerts WebSocket: ${alertsConnected ? "connected" : "disconnected"}`}>
        <span className={`w-1.5 h-1.5 rounded-full ${alertsConnected ? "bg-green-500" : "bg-red-500"}`} />
        Alerts
      </span>

      <span className="w-px h-3 bg-white/10" />

      {/* Wallet balance */}
      <span>{walletBalance ? `$${walletBalance} USDC` : "\u2014"}</span>

      <span className="w-px h-3 bg-white/10" />

      {/* Copy-trade summary */}
      <CopyTradeSummaryBar activeSessions={summary?.active_sessions ?? 0} totalPnl={summary?.total_pnl ?? 0} />

      <span className="w-px h-3 bg-white/10" />

      {/* Last order */}
      <span>
        Last order: {lastOrderTime ? formatRelativeTime(lastOrderTime) : "\u2014"}
      </span>

      <div className="flex-1" />

      <button
        className="text-[10px] text-[var(--text-secondary)] hover:text-[var(--text-primary)] font-mono"
        onClick={requestOpenCommandPalette}
        title="Open commands"
      >
        Commands {commandHint}
      </button>
    </div>
  );
}
