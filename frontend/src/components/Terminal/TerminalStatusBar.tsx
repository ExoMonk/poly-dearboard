import { useMemo } from "react";
import { useTerminalState } from "./TerminalProvider";

function formatRelativeTime(ts: number): string {
  const diff = Math.floor((Date.now() - ts) / 1000);
  if (diff < 60) return `${diff}s ago`;
  if (diff < 3600) return `${Math.floor(diff / 60)}m ago`;
  return `${Math.floor(diff / 3600)}h ago`;
}

export function TerminalStatusBar() {
  const { logs, walletBalance } = useTerminalState();

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
      {/* Connection indicator — always green for now (no WS connection in spec 12) */}
      <span className="flex items-center gap-1.5">
        <span className="w-1.5 h-1.5 rounded-full bg-green-500" />
        Connected
      </span>

      <span className="w-px h-3 bg-white/10" />

      {/* Wallet balance */}
      <span>{walletBalance ? `$${walletBalance} USDC` : "\u2014"}</span>

      <span className="w-px h-3 bg-white/10" />

      {/* Last order */}
      <span>
        Last order: {lastOrderTime ? formatRelativeTime(lastOrderTime) : "\u2014"}
      </span>
    </div>
  );
}
