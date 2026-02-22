import { useTerminalState, useTerminalDispatch } from "./TerminalProvider";
import type { WalletStatus } from "../../types";

const WALLET_DOT_COLORS: Record<WalletStatus, string> = {
  none: "bg-gray-500",
  setup: "bg-yellow-500",
  funded: "bg-yellow-500",
  active: "bg-green-500",
};

export function TerminalHeader() {
  const { height, walletStatus, activeSessions } = useTerminalState();
  const { toggle, setHeight } = useTerminalDispatch();

  return (
    <div
      className="flex items-center h-10 px-3 gap-3 cursor-pointer select-none border-b border-white/5"
      onClick={toggle}
      onDoubleClick={() => setHeight(height === "full" ? "half" : "full")}
    >
      {/* Drag handle / chevron */}
      <svg
        className={`w-3.5 h-3.5 text-[var(--text-muted)] transition-transform ${
          height !== "collapsed" ? "rotate-180" : ""
        }`}
        viewBox="0 0 24 24"
        fill="none"
        stroke="currentColor"
        strokeWidth="2"
        strokeLinecap="round"
        strokeLinejoin="round"
      >
        <polyline points="18 15 12 9 6 15" />
      </svg>

      <span className="text-xs font-medium text-[var(--text-secondary)] tracking-wide uppercase">
        Terminal
      </span>

      {/* Wallet status dot */}
      <div className={`w-2 h-2 rounded-full ${WALLET_DOT_COLORS[walletStatus]}`} />

      {/* Session badge */}
      {activeSessions > 0 && (
        <span className="text-[10px] px-1.5 py-0.5 rounded-full bg-[var(--accent-blue)]/20 text-[var(--accent-blue)] font-medium">
          {activeSessions} session{activeSessions !== 1 ? "s" : ""}
        </span>
      )}

      <div className="flex-1" />

      {/* Window controls */}
      <div className="flex items-center gap-1" onClick={(e) => e.stopPropagation()}>
        {/* Minimize */}
        <button
          className="p-1 rounded hover:bg-white/5 text-[var(--text-muted)]"
          onClick={() => setHeight("collapsed")}
          title="Collapse"
        >
          <svg className="w-3.5 h-3.5" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
            <line x1="5" y1="12" x2="19" y2="12" />
          </svg>
        </button>
        {/* Maximize */}
        <button
          className="p-1 rounded hover:bg-white/5 text-[var(--text-muted)]"
          onClick={() => setHeight(height === "full" ? "half" : "full")}
          title={height === "full" ? "Restore" : "Maximize"}
        >
          <svg className="w-3.5 h-3.5" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
            <rect x="3" y="3" width="18" height="18" rx="2" />
          </svg>
        </button>
        {/* Close */}
        <button
          className="p-1 rounded hover:bg-white/5 text-[var(--text-muted)]"
          onClick={() => setHeight("collapsed")}
          title="Close"
        >
          <svg className="w-3.5 h-3.5" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
            <line x1="18" y1="6" x2="6" y2="18" />
            <line x1="6" y1="6" x2="18" y2="18" />
          </svg>
        </button>
      </div>
    </div>
  );
}
