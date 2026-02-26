import { motion, AnimatePresence } from "motion/react";
import useSignalFeed from "../../hooks/useSignalFeed";
import { useSessions } from "../../hooks/useCopyTrade";
import { useTerminal } from "../../components/Terminal/TerminalProvider";
import { requestOpenCreateSession } from "../../components/Terminal/CreateSessionModal";
import ListSelector from "./ListSelector";
import { useState, useEffect, useRef, useCallback } from "react";
import { Link } from "react-router-dom";
import { shortenAddress, formatUsd, polygonscanTx } from "../../lib/format";
import { panelVariants, alertCardVariants } from "../../lib/motion";
import { SectionHeader } from "./shared";
import type { SignalTrade } from "../../types";

const DEFAULT_TOP_N = 20;
const MAX_RENDER_TRADES = 200;

export default function SignalFeed() {
  const [listId, setListId] = useState<string | null>(null);
  const [displayTrades, setDisplayTrades] = useState<SignalTrade[]>([]);
  const [highlightIds, setHighlightIds] = useState<Set<string>>(new Set());

  const seenTradeIdsRef = useRef<Set<string>>(new Set());
  const highlightTimerRef = useRef<number | null>(null);

  const { data: sessions } = useSessions();
  const { setActiveTab, setHeight } = useTerminal();
  const hasActiveSession = sessions?.some((s) => s.status !== "stopped");
  const { trades, alerts, connected, isLagging } = useSignalFeed({
    listId,
    topN: listId ? undefined : DEFAULT_TOP_N,
  });

  const enqueueHighlights = useCallback((ids: string[]) => {
    if (ids.length === 0) return;
    setHighlightIds((prev) => {
      const next = new Set(prev);
      ids.forEach((id) => next.add(id));
      return next;
    });
    if (highlightTimerRef.current) {
      window.clearTimeout(highlightTimerRef.current);
    }
    highlightTimerRef.current = window.setTimeout(() => {
      setHighlightIds(new Set());
      highlightTimerRef.current = null;
    }, 2000);
  }, []);

  useEffect(() => {
    return () => {
      if (highlightTimerRef.current) {
        window.clearTimeout(highlightTimerRef.current);
      }
    };
  }, []);

  useEffect(() => {
    seenTradeIdsRef.current = new Set();
    setDisplayTrades([]);
    setHighlightIds(new Set());
  }, [listId]);

  useEffect(() => {
    const incoming = trades.filter((trade) => !seenTradeIdsRef.current.has(trade.tx_hash));
    if (incoming.length === 0) return;

    incoming.forEach((trade) => seenTradeIdsRef.current.add(trade.tx_hash));

    setDisplayTrades((prev) => [...incoming, ...prev].slice(0, MAX_RENDER_TRADES));
    enqueueHighlights(incoming.map((trade) => trade.tx_hash));
  }, [trades, enqueueHighlights]);

  const visibleTrades = displayTrades;

  return (
    <div className="space-y-4">
      {/* Controls */}
      <motion.div
        variants={panelVariants}
        initial="initial"
        animate="animate"
        className="glass p-5 gradient-border-top"
      >
        <div className="flex items-center justify-between flex-wrap gap-4">
          <ListSelector selectedId={listId} onSelect={setListId} />
          <div className="flex items-center gap-3">
            {!hasActiveSession && (
              <button
                className="text-xs font-semibold px-3 py-1.5 rounded bg-[var(--neon-green)]/10 text-[var(--neon-green)] border border-[var(--neon-green)]/30 hover:bg-[var(--neon-green)]/20 transition-colors"
                onClick={requestOpenCreateSession}
              >
                Start Copy-Trade
              </button>
            )}
            {hasActiveSession && (
              <button
                className="text-xs font-semibold px-3 py-1.5 rounded bg-blue-500/10 text-blue-400 border border-blue-500/30 hover:bg-blue-500/20 transition-colors"
                onClick={() => {
                  setActiveTab("sessions");
                  setHeight("half");
                }}
              >
                View Session
              </button>
            )}
            {isLagging && (
              <span className="text-xs font-semibold text-[var(--accent-orange)] bg-[var(--accent-orange)]/10 px-2 py-1 rounded-full">
                Signal lag — some trades may be delayed
              </span>
            )}
            <span
              className={`w-2 h-2 rounded-full ${connected ? "bg-[var(--neon-green)] shadow-[0_0_6px_var(--neon-green)]" : "bg-[var(--text-secondary)]"}`}
            />
            <span className="text-xs text-[var(--text-secondary)]">
              {connected ? "Live" : "Connecting..."}
            </span>
          </div>
        </div>
      </motion.div>

      <div className="grid grid-cols-1 lg:grid-cols-3 gap-4">
          {/* Trade stream — 2/3 width */}
          <div className="lg:col-span-2 space-y-2">
            <SectionHeader dot="bg-[var(--accent-blue)] shadow-[0_0_6px_var(--accent-blue)]">
              Live Trades ({visibleTrades.length})
            </SectionHeader>
            {visibleTrades.length === 0 ? (
              <div className="glass p-8 text-center">
                <p className="text-[var(--text-secondary)] text-sm">Waiting for trades...</p>
              </div>
            ) : (
              <div className="space-y-1.5 max-h-[600px] overflow-y-auto">
                <AnimatePresence initial={false}>
                  {visibleTrades.map((t) => (
                    <motion.div
                      key={t.tx_hash}
                      variants={alertCardVariants}
                      initial="initial"
                      animate="animate"
                      exit="exit"
                      className={`glass flex items-center gap-3 px-4 py-3 text-sm ${highlightIds.has(t.tx_hash) ? "delta-flash-neutral" : ""}`}
                    >
                      <span
                        className={`text-[10px] font-bold uppercase px-2 py-0.5 rounded-full tracking-wide ${
                          t.side.toLowerCase() === "buy"
                            ? "text-[var(--neon-green)] bg-[var(--neon-green)]/10 shadow-[0_0_6px_rgba(0,255,136,0.15)]"
                            : "text-[var(--neon-red)] bg-[var(--neon-red)]/10 shadow-[0_0_6px_rgba(255,51,102,0.15)]"
                        }`}
                      >
                        {t.side}
                      </span>
                      <Link
                        to={`/trader/${t.trader}`}
                        className="font-mono text-[var(--accent-blue)] hover:text-white transition-colors shrink-0"
                      >
                        {shortenAddress(t.trader)}
                      </Link>
                      <Link
                        to={`/market/${t.asset_id}`}
                        className="text-[var(--text-secondary)] hover:text-[var(--text-primary)] truncate flex-1 min-w-0 transition-colors"
                      >
                        {t.question ?? t.asset_id.slice(0, 12)}
                        {t.outcome && (
                          <span className="opacity-50 ml-1">· {t.outcome}</span>
                        )}
                      </Link>
                      <span className="font-mono font-bold text-[var(--text-primary)] shrink-0">
                        {formatUsd(t.usdc_amount)}
                      </span>
                      <a
                        href={polygonscanTx(t.tx_hash)}
                        target="_blank"
                        rel="noopener noreferrer"
                        title="View on Polygonscan"
                        className="inline-flex items-center gap-0.5 px-1.5 py-0.5 rounded text-[10px] font-semibold text-[var(--neon-green)]/50 hover:text-[var(--neon-green)] hover:bg-[var(--neon-green)]/10 transition-colors shrink-0"
                      >
                        Scan
                        <svg viewBox="0 0 16 16" fill="currentColor" className="w-3 h-3">
                          <path d="M3.75 2h3.5a.75.75 0 0 1 0 1.5h-3.5a.25.25 0 0 0-.25.25v8.5c0 .138.112.25.25.25h8.5a.25.25 0 0 0 .25-.25v-3.5a.75.75 0 0 1 1.5 0v3.5A1.75 1.75 0 0 1 12.25 14h-8.5A1.75 1.75 0 0 1 2 12.25v-8.5C2 2.784 2.784 2 3.75 2Zm6.854-1h4.146a.25.25 0 0 1 .25.25v4.146a.25.25 0 0 1-.427.177L13.03 4.03 9.28 7.78a.751.751 0 0 1-1.042-.018.751.751 0 0 1-.018-1.042l3.75-3.75-1.543-1.543A.25.25 0 0 1 10.604 1Z" />
                        </svg>
                      </a>
                    </motion.div>
                  ))}
                </AnimatePresence>
              </div>
            )}
          </div>

          {/* Convergence alerts — 1/3 width */}
          <div className="space-y-2">
            <SectionHeader dot="bg-[var(--accent-orange)] shadow-[0_0_6px_var(--accent-orange)]">
              Convergence Alerts ({alerts.length})
            </SectionHeader>
            {alerts.length === 0 ? (
              <div className="glass p-8 text-center">
                <p className="text-[var(--text-secondary)] text-sm">
                  Alerts fire when multiple watched traders enter the same market.
                </p>
              </div>
            ) : (
              <div className="space-y-2 max-h-[600px] overflow-y-auto">
                <AnimatePresence initial={false}>
                  {alerts.map((a, i) => (
                    <motion.div
                      key={`${a.asset_id}-${i}`}
                      variants={alertCardVariants}
                      initial="initial"
                      animate="animate"
                      exit="exit"
                      className="glass px-4 py-3 border border-[var(--accent-orange)]/20 shadow-[0_0_12px_rgba(249,115,22,0.08)]"
                    >
                      <div className="flex items-center gap-2 mb-2">
                        <span className="text-xs font-bold text-[var(--accent-orange)] uppercase">
                          Convergence
                        </span>
                        <span
                          className={`text-xs font-bold px-1.5 py-0.5 rounded ${
                            a.side.toLowerCase() === "buy"
                              ? "text-[var(--neon-green)] bg-[var(--neon-green)]/10"
                              : "text-[var(--neon-red)] bg-[var(--neon-red)]/10"
                          }`}
                        >
                          {a.side}
                        </span>
                      </div>
                      <p className="text-sm text-[var(--text-primary)] mb-2 line-clamp-2">
                        {a.question ?? a.asset_id.slice(0, 20)}
                        {a.outcome && <span className="text-[var(--text-secondary)]"> · {a.outcome}</span>}
                      </p>
                      <div className="flex flex-wrap gap-1">
                        {a.traders.map((addr) => (
                          <Link
                            key={addr}
                            to={`/trader/${addr}`}
                            className="text-xs font-mono text-[var(--accent-blue)] hover:text-white transition-colors"
                          >
                            {shortenAddress(addr)}
                          </Link>
                        ))}
                      </div>
                    </motion.div>
                  ))}
                </AnimatePresence>
              </div>
            )}
          </div>
        </div>
    </div>
  );
}
