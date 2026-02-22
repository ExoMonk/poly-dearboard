import { useEffect, useRef, useState, useCallback } from "react";
import { useTerminalState, useTerminalDispatch } from "./TerminalProvider";
import type { LogEntry } from "../../types";

const LEVEL_COLORS: Record<LogEntry["level"], string> = {
  info: "text-[var(--text-muted)]",
  warn: "text-yellow-400",
  error: "text-red-400",
  success: "text-green-400",
};

const LEVEL_BADGE_BG: Record<LogEntry["level"], string> = {
  info: "bg-white/10",
  warn: "bg-yellow-500/20",
  error: "bg-red-500/20",
  success: "bg-green-500/20",
};

function formatTime(ts: number): string {
  const d = new Date(ts);
  return d.toLocaleTimeString("en-US", { hour12: false, hour: "2-digit", minute: "2-digit", second: "2-digit" });
}

function LogEntryRow({ entry }: { entry: LogEntry }) {
  const [expanded, setExpanded] = useState(false);
  const hasMeta = entry.meta && Object.keys(entry.meta).length > 0;

  return (
    <div className="group px-3 py-1 hover:bg-white/[0.02] text-xs font-mono">
      <div className="flex items-start gap-2">
        <span className="text-[var(--text-muted)] shrink-0">{formatTime(entry.timestamp)}</span>
        <span
          className={`shrink-0 px-1 rounded text-[10px] uppercase font-semibold ${LEVEL_COLORS[entry.level]} ${LEVEL_BADGE_BG[entry.level]}`}
        >
          {entry.level}
        </span>
        <span className="text-[var(--text-primary)] break-all">{entry.message}</span>
        {hasMeta && (
          <button
            className="shrink-0 text-[var(--text-muted)] hover:text-[var(--text-secondary)] opacity-0 group-hover:opacity-100 transition-opacity"
            onClick={() => setExpanded(!expanded)}
          >
            {expanded ? "[-]" : "[+]"}
          </button>
        )}
      </div>
      {expanded && entry.meta && (
        <div className="flex flex-wrap gap-1.5 mt-1 ml-[72px]">
          {Object.entries(entry.meta).map(([key, value]) => {
            const isTxHash = key === "tx_hash" && value.startsWith("0x");
            return (
              <span key={key} className="inline-flex items-center gap-1 px-1.5 py-0.5 rounded bg-white/5 text-[10px] text-[var(--text-muted)]">
                <span className="text-[var(--text-secondary)]">{key}:</span>
                {isTxHash ? (
                  <a
                    href={`https://polygonscan.com/tx/${value}`}
                    target="_blank"
                    rel="noopener noreferrer"
                    className="text-[var(--accent-blue)] hover:underline"
                    onClick={(e) => e.stopPropagation()}
                  >
                    {value.slice(0, 10)}...
                  </a>
                ) : (
                  <span>{value}</span>
                )}
              </span>
            );
          })}
        </div>
      )}
    </div>
  );
}

export function TerminalLogs() {
  const { logs } = useTerminalState();
  const { clearLogs } = useTerminalDispatch();
  const scrollRef = useRef<HTMLDivElement>(null);
  const [scrollLock, setScrollLock] = useState(false);

  // Auto-scroll to bottom
  useEffect(() => {
    if (!scrollLock && scrollRef.current) {
      scrollRef.current.scrollTop = scrollRef.current.scrollHeight;
    }
  }, [logs.length, scrollLock]);

  const handleScroll = useCallback(() => {
    if (!scrollRef.current) return;
    const { scrollTop, scrollHeight, clientHeight } = scrollRef.current;
    // If user scrolled up more than 50px from bottom, enable scroll lock
    const atBottom = scrollHeight - scrollTop - clientHeight < 50;
    if (!atBottom && !scrollLock) setScrollLock(true);
    if (atBottom && scrollLock) setScrollLock(false);
  }, [scrollLock]);

  return (
    <div className="flex flex-col h-full">
      {/* Toolbar */}
      <div className="flex items-center justify-between px-3 py-1.5 border-b border-white/5">
        <span className="text-[10px] text-[var(--text-muted)]">{logs.length} entries</span>
        <div className="flex items-center gap-2">
          <button
            className={`text-[10px] px-1.5 py-0.5 rounded ${
              scrollLock ? "bg-yellow-500/20 text-yellow-400" : "text-[var(--text-muted)] hover:text-[var(--text-secondary)]"
            }`}
            onClick={() => setScrollLock(!scrollLock)}
          >
            {scrollLock ? "Scroll locked" : "Auto-scroll"}
          </button>
          <button
            className="text-[10px] text-[var(--text-muted)] hover:text-[var(--text-secondary)]"
            onClick={clearLogs}
          >
            Clear
          </button>
        </div>
      </div>

      {/* Log list */}
      <div ref={scrollRef} className="flex-1 overflow-y-auto" onScroll={handleScroll}>
        {logs.length === 0 ? (
          <div className="flex items-center justify-center h-full text-xs text-[var(--text-muted)]">
            No log entries yet.
          </div>
        ) : (
          logs.map((entry) => <LogEntryRow key={entry.id} entry={entry} />)
        )}
      </div>
    </div>
  );
}
