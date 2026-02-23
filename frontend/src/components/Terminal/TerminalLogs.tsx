import { useEffect, useRef, useState, useMemo, useCallback } from "react";
import { useTerminalState, useTerminalDispatch } from "./TerminalProvider";
import type { LogEntry, LogSource } from "../../types";

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

const SOURCE_COLORS: Record<LogSource, { active: string; label: string }> = {
  wallet: { active: "bg-blue-500/20 text-blue-400", label: "WAL" },
  copytrade: { active: "bg-green-500/20 text-green-400", label: "CT" },
  alert: { active: "bg-yellow-500/20 text-yellow-400", label: "ALT" },
};

const ALL_LEVELS: LogEntry["level"][] = ["info", "warn", "error", "success"];
const ALL_SOURCES: LogSource[] = ["wallet", "copytrade", "alert"];
const INACTIVE_CHIP = "bg-white/5 text-[var(--text-muted)] opacity-50";

function formatTime(ts: number): string {
  const d = new Date(ts);
  return d.toLocaleTimeString("en-US", { hour12: false, hour: "2-digit", minute: "2-digit", second: "2-digit" });
}

function toggleSet<T>(set: Set<T>, value: T): Set<T> | null {
  const next = new Set(set);
  if (next.has(value)) {
    next.delete(value);
    if (next.size === 0) return null; // prevent empty
  } else {
    next.add(value);
  }
  return next;
}

function LogEntryRow({ entry }: { entry: LogEntry }) {
  const [expanded, setExpanded] = useState(false);
  const hasMeta = entry.meta && Object.keys(entry.meta).length > 0;
  const source = entry.source ?? "wallet";

  return (
    <div className="group px-3 py-1 hover:bg-white/[0.02] text-xs font-mono">
      <div className="flex items-start gap-2">
        <span className="text-[var(--text-muted)] shrink-0">{formatTime(entry.timestamp)}</span>
        <span
          className={`shrink-0 px-1 rounded text-[10px] uppercase font-semibold ${SOURCE_COLORS[source].active}`}
        >
          {SOURCE_COLORS[source].label}
        </span>
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
        <div className="flex flex-wrap gap-1.5 mt-1 ml-[100px]">
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
  const [levelFilter, setLevelFilter] = useState<Set<LogEntry["level"]>>(new Set(ALL_LEVELS));
  const [sourceFilter, setSourceFilter] = useState<Set<LogSource>>(new Set(ALL_SOURCES));

  const filteredLogs = useMemo(
    () => logs.filter((l) => levelFilter.has(l.level) && sourceFilter.has(l.source ?? "wallet")),
    [logs, levelFilter, sourceFilter],
  );

  const isFiltering = filteredLogs.length !== logs.length;

  // Auto-scroll to bottom
  useEffect(() => {
    if (!scrollLock && scrollRef.current) {
      scrollRef.current.scrollTop = scrollRef.current.scrollHeight;
    }
  }, [filteredLogs.length, scrollLock]);

  const handleScroll = useCallback(() => {
    if (!scrollRef.current) return;
    const { scrollTop, scrollHeight, clientHeight } = scrollRef.current;
    const atBottom = scrollHeight - scrollTop - clientHeight < 50;
    if (!atBottom && !scrollLock) setScrollLock(true);
    if (atBottom && scrollLock) setScrollLock(false);
  }, [scrollLock]);

  return (
    <div className="flex flex-col h-full">
      {/* Toolbar */}
      <div className="flex flex-wrap items-center gap-2 px-3 py-1.5 border-b border-white/5">
        <span className="text-[10px] text-[var(--text-muted)]">
          {isFiltering ? `${filteredLogs.length}/${logs.length}` : logs.length} entries
        </span>

        <span className="text-white/10">|</span>

        {/* Level filter chips */}
        <div className="flex items-center gap-1">
          {ALL_LEVELS.map((level) => {
            const active = levelFilter.has(level);
            return (
              <button
                key={level}
                className={`text-[10px] px-1.5 py-0.5 rounded font-medium uppercase transition-opacity ${
                  active ? `${LEVEL_COLORS[level]} ${LEVEL_BADGE_BG[level]}` : INACTIVE_CHIP
                }`}
                onClick={() => {
                  const next = toggleSet(levelFilter, level);
                  if (next) setLevelFilter(next);
                }}
              >
                {level}
              </button>
            );
          })}
        </div>

        <span className="text-white/10">|</span>

        {/* Source filter chips */}
        <div className="flex items-center gap-1">
          {ALL_SOURCES.map((source) => {
            const active = sourceFilter.has(source);
            return (
              <button
                key={source}
                className={`text-[10px] px-1.5 py-0.5 rounded font-medium transition-opacity ${
                  active ? SOURCE_COLORS[source].active : INACTIVE_CHIP
                }`}
                onClick={() => {
                  const next = toggleSet(sourceFilter, source);
                  if (next) setSourceFilter(next);
                }}
              >
                {source}
              </button>
            );
          })}
        </div>

        <div className="flex items-center gap-2 ml-auto">
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
        {filteredLogs.length === 0 ? (
          <div className="flex items-center justify-center h-full text-xs text-[var(--text-muted)]">
            {logs.length === 0 ? "No log entries yet." : "No logs match current filters."}
          </div>
        ) : (
          filteredLogs.map((entry) => <LogEntryRow key={entry.id} entry={entry} />)
        )}
      </div>
    </div>
  );
}
