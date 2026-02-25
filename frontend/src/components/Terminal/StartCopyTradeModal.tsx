import { useState, useEffect, useRef } from "react";
import { motion, AnimatePresence } from "motion/react";
import { useCreateSession } from "../../hooks/useCopyTrade";
import { useTraderLists } from "../../hooks/useTraderLists";
import { useTerminal } from "./TerminalProvider";
import type { CopyOrderType, CreateSessionRequest } from "../../types";

interface Props {
  isOpen: boolean;
  onClose: () => void;
}

export function StartCopyTradeModal({ isOpen, onClose }: Props) {
  const create = useCreateSession();
  const { data: lists } = useTraderLists();
  const { setActiveTab, setHeight } = useTerminal();
  const overlayRef = useRef<HTMLDivElement>(null);

  const [source, setSource] = useState<"top_n" | "list">("top_n");
  const [topN, setTopN] = useState(10);
  const [listId, setListId] = useState("");
  const [capital, setCapital] = useState(5);
  const [copyPct, setCopyPct] = useState(50);
  const [maxPosition, setMaxPosition] = useState(100);
  const [maxSlippage, setMaxSlippage] = useState(200);
  const [orderType, setOrderType] = useState<CopyOrderType>("FOK");
  const [maxLossPct, setMaxLossPct] = useState(20);
  const [simulate, setSimulate] = useState(true);
  const [showLiveConfirm, setShowLiveConfirm] = useState(false);
  const [error, setError] = useState("");

  // Load backtest config from sessionStorage if available
  useEffect(() => {
    if (!isOpen) return;
    try {
      const raw = sessionStorage.getItem("backtest_config");
      if (raw) {
        const cfg = JSON.parse(raw);
        if (cfg.capital) setCapital(cfg.capital);
        if (cfg.copy_pct) setCopyPct(cfg.copy_pct * 100);
        if (cfg.max_position_usdc) setMaxPosition(cfg.max_position_usdc);
        if (cfg.max_slippage_bps) setMaxSlippage(cfg.max_slippage_bps);
        if (cfg.list_id) { setSource("list"); setListId(cfg.list_id); }
        if (cfg.top_n) { setSource("top_n"); setTopN(cfg.top_n); }
      }
    } catch { /* ignore */ }
  }, [isOpen]);

  const handleSubmit = () => {
    setError("");
    if (!simulate && (capital < 1 || capital > 10)) {
      setError("Live mode capital must be between $1 and $10 USDC during beta.");
      return;
    }
    const body: CreateSessionRequest = {
      copy_pct: copyPct / 100,
      max_position_usdc: maxPosition,
      max_slippage_bps: maxSlippage,
      order_type: orderType,
      initial_capital: capital,
      simulate,
      max_loss_pct: maxLossPct,
      ...(source === "top_n" ? { top_n: topN } : { list_id: listId }),
    };
    create.mutate(body, {
      onSuccess: () => {
        onClose();
        setActiveTab("sessions");
        setHeight("half");
      },
      onError: (e) => setError(e.message),
    });
  };

  const inputCls = "w-full bg-[var(--surface-2)] border border-[var(--border-subtle)] rounded px-2 py-1.5 text-xs font-mono text-[var(--text-primary)] focus:outline-none focus:border-[var(--neon-green)]/50";
  const labelCls = "text-[10px] text-[var(--text-muted)] uppercase tracking-wider mb-1";

  return (
    <AnimatePresence>
      {isOpen && (
        <motion.div
          ref={overlayRef}
          className="fixed inset-0 z-50 flex items-center justify-center bg-black/60 backdrop-blur-sm"
          initial={{ opacity: 0 }}
          animate={{ opacity: 1 }}
          exit={{ opacity: 0 }}
          onClick={(e) => { if (e.target === overlayRef.current) onClose(); }}
        >
          <motion.div
            className="glass border border-[var(--border-glow)] rounded-lg p-5 w-full max-w-md mx-4"
            initial={{ scale: 0.95, opacity: 0 }}
            animate={{ scale: 1, opacity: 1 }}
            exit={{ scale: 0.95, opacity: 0 }}
            transition={{ type: "spring", stiffness: 300, damping: 30 }}
          >
            <div className="flex items-center justify-between mb-4">
              <h3 className="text-sm font-semibold text-[var(--text-primary)]">{simulate ? "Start Copy-Trade Simulation" : "Start Live Copy-Trade"}</h3>
              <button onClick={onClose} className="text-xs text-[var(--text-muted)] hover:text-[var(--text-primary)]">Cancel</button>
            </div>

            <div className="space-y-3">
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
                  <div className={labelCls}>Top N Traders</div>
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
                  <div className={labelCls}>Capital (USDC){!simulate && " (max $10 beta)"}</div>
                  <input type="number" className={inputCls} value={capital} min={1} max={simulate ? undefined : 10} onChange={(e) => setCapital(Number(e.target.value))} />
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

              {/* Mode: Simulate / Live */}
              <div>
                <div className={labelCls}>Mode</div>
                <div className="flex gap-1.5">
                  <button
                    className={`px-2 py-1 text-xs rounded border ${simulate ? "bg-[var(--neon-green)]/10 text-[var(--neon-green)] border-[var(--neon-green)]/30" : "bg-[var(--surface-2)] text-[var(--text-muted)] border-[var(--border-subtle)]"}`}
                    onClick={() => { setSimulate(true); setShowLiveConfirm(false); }}
                  >
                    Simulate
                  </button>
                  <button
                    className={`px-2 py-1 text-xs rounded border ${!simulate ? "bg-red-500/10 text-red-400 border-red-500/30" : "bg-[var(--surface-2)] text-[var(--text-muted)] border-[var(--border-subtle)]"}`}
                    onClick={() => { if (simulate) setShowLiveConfirm(true); }}
                  >
                    Live
                  </button>
                </div>
              </div>

              {/* Live mode confirmation */}
              {showLiveConfirm && simulate && (
                <div className="border border-red-500/30 rounded p-3 bg-red-500/5">
                  <p className="text-[10px] text-red-300 mb-2">
                    Live mode places real orders on Polymarket CLOB using your wallet funds. This is irreversible. Ensure your wallet is funded and CLOB credentials are derived.
                  </p>
                  <div className="flex gap-2">
                    <button
                      className="px-2 py-1 text-[10px] rounded border border-red-500/30 bg-red-500/10 text-red-400 hover:bg-red-500/20"
                      onClick={() => { setSimulate(false); setShowLiveConfirm(false); if (capital > 10) setCapital(10); }}
                    >
                      Confirm Live Mode
                    </button>
                    <button
                      className="px-2 py-1 text-[10px] rounded border border-[var(--border-subtle)] text-[var(--text-muted)] hover:text-[var(--text-primary)]"
                      onClick={() => setShowLiveConfirm(false)}
                    >
                      Cancel
                    </button>
                  </div>
                </div>
              )}

              {error && <div className="text-xs text-red-400">{error}</div>}

              <button
                className={`w-full py-2 text-xs font-semibold rounded border transition-colors disabled:opacity-50 ${simulate ? "bg-[var(--neon-green)]/10 text-[var(--neon-green)] border-[var(--neon-green)]/30 hover:bg-[var(--neon-green)]/20" : "bg-red-500/10 text-red-400 border-red-500/30 hover:bg-red-500/20"}`}
                onClick={handleSubmit}
                disabled={create.isPending || (source === "list" && !listId)}
              >
                {create.isPending ? "Starting..." : simulate ? "Start Simulation" : "Start Live Trading"}
              </button>
            </div>
          </motion.div>
        </motion.div>
      )}
    </AnimatePresence>
  );
}
