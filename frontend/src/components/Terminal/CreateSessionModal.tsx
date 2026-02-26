import { useState, useEffect, useRef, useCallback } from "react";
import { motion, AnimatePresence } from "motion/react";
import { useCreateSession } from "../../hooks/useCopyTrade";
import { useTraderLists } from "../../hooks/useTraderLists";
import { useWallets } from "../../hooks/useWallet";
import { useTerminal } from "./TerminalProvider";
import type { CopyOrderType, CreateSessionRequest, CreateSessionPrefill } from "../../types";

/* ── event bus ─────────────────────────────────────────────── */
const OPEN_EVENT = "session:open-create-modal";

export function requestOpenCreateSession(prefill?: CreateSessionPrefill) {
  window.dispatchEvent(new CustomEvent(OPEN_EVENT, { detail: prefill }));
}

/* ── pill toggle ───────────────────────────────────────────── */
function Pill<T extends string>({
  options,
  value,
  onChange,
  color = "green",
}: {
  options: { value: T; label: string }[];
  value: T;
  onChange: (v: T) => void;
  color?: "green" | "red" | "blue";
}) {
  const colors = {
    green: "bg-[var(--neon-green)]/15 text-[var(--neon-green)] shadow-[0_0_8px_rgba(0,255,136,0.12)]",
    red: "bg-red-500/15 text-red-400 shadow-[0_0_8px_rgba(239,68,68,0.12)]",
    blue: "bg-[var(--accent-blue)]/15 text-[var(--accent-blue)] shadow-[0_0_8px_rgba(59,130,246,0.12)]",
  };
  const inactive = "text-[var(--text-secondary)] hover:text-[var(--text-primary)]";

  return (
    <div className="inline-flex rounded-xl bg-white/[0.03] border border-white/[0.06] p-0.5 gap-0.5">
      {options.map((o) => (
        <button
          key={o.value}
          type="button"
          className={`px-3.5 py-1.5 text-xs font-medium rounded-[10px] transition-all duration-150 ${
            value === o.value ? colors[color] : inactive
          }`}
          onClick={() => onChange(o.value)}
        >
          {o.label}
        </button>
      ))}
    </div>
  );
}

/* ── styled inputs ─────────────────────────────────────────── */
const inputCls =
  "w-full bg-white/[0.04] border border-white/[0.08] rounded-xl px-3 py-2 text-sm font-mono text-[var(--text-primary)] placeholder:text-[var(--text-secondary)] focus:outline-none focus:border-[var(--accent-blue)]/40 focus:bg-white/[0.06] transition-all duration-150";

const labelCls = "text-[11px] text-[var(--text-secondary)] font-medium tracking-wide mb-1.5 block";

const selectCls = `${inputCls} appearance-none cursor-pointer`;

/* ── section wrapper ───────────────────────────────────────── */
function Section({ label, children }: { label: string; children: React.ReactNode }) {
  return (
    <div className="space-y-3">
      <div className="text-[10px] uppercase tracking-[0.1em] text-[var(--text-secondary)]/60 font-semibold">
        {label}
      </div>
      {children}
    </div>
  );
}

/* ── main modal ────────────────────────────────────────────── */
export function CreateSessionModal() {
  const create = useCreateSession();
  const { data: lists } = useTraderLists();
  const { data: wallets } = useWallets();
  const { setActiveTab, setHeight } = useTerminal();
  const overlayRef = useRef<HTMLDivElement>(null);

  const [open, setOpen] = useState(false);

  // Core fields
  const [source, setSource] = useState<"top_n" | "list">("top_n");
  const [topN, setTopN] = useState(10);
  const [listId, setListId] = useState("");
  const [capital, setCapital] = useState(1000);
  const [copyPct, setCopyPct] = useState(50);
  const [maxPosition, setMaxPosition] = useState(100);
  const [maxSlippage, setMaxSlippage] = useState(200);
  const [orderType, setOrderType] = useState<CopyOrderType>("FOK");
  const [maxLossPct, setMaxLossPct] = useState(20);
  const [simulate, setSimulate] = useState(true);
  const [showLiveConfirm, setShowLiveConfirm] = useState(false);
  const [walletId, setWalletId] = useState("");

  // Advanced
  const [showAdvanced, setShowAdvanced] = useState(false);
  const [minSourceUsdc, setMinSourceUsdc] = useState(50);
  const [utilizationCap, setUtilizationCap] = useState(100);
  const [maxOpenPositions, setMaxOpenPositions] = useState(10);
  const [takeProfitPct, setTakeProfitPct] = useState<number | "">("");
  const [stopLossPct, setStopLossPct] = useState<number | "">("");
  const [mirrorClose, setMirrorClose] = useState(true);
  const [healthInterval, setHealthInterval] = useState(30);
  const [maxSourcePrice, setMaxSourcePrice] = useState(95);
  const [minSourcePrice, setMinSourcePrice] = useState(5);

  const [error, setError] = useState("");
  const prefillRef = useRef<CreateSessionPrefill | null>(null);

  const liveWallets = (wallets ?? []).filter((w) => w.has_clob_credentials);

  /* ── open / close ───────────────────────────────────────── */
  const openModal = useCallback((prefill?: CreateSessionPrefill) => {
    prefillRef.current = prefill ?? null;
    setOpen(true);

    // Apply prefill defaults (takes priority over backtest config)
    if (prefill?.defaults) {
      const d = prefill.defaults;
      if (d.simulationMode) setSimulate(d.simulationMode === "simulate");
      if (d.copySizePercent) setCopyPct(d.copySizePercent);
      if (d.maxPositionUsd) setMaxPosition(d.maxPositionUsd);
      if (d.minSourceTradeUsd) setMinSourceUsdc(d.minSourceTradeUsd);
    }

    // Fallback: backtest config from sessionStorage
    if (!prefill) {
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
    }
  }, []);

  const closeModal = useCallback(() => {
    setOpen(false);
    setError("");
    setShowLiveConfirm(false);
    prefillRef.current = null;
  }, []);

  useEffect(() => {
    const handler = (e: Event) => openModal((e as CustomEvent).detail);
    window.addEventListener(OPEN_EVENT, handler);
    return () => window.removeEventListener(OPEN_EVENT, handler);
  }, [openModal]);

  useEffect(() => {
    if (open) {
      document.body.style.overflow = "hidden";
    } else {
      document.body.style.overflow = "";
    }
    return () => { document.body.style.overflow = ""; };
  }, [open]);

  /* ── submit ─────────────────────────────────────────────── */
  const handleSubmit = () => {
    setError("");
    if (!simulate && capital > 10) {
      setError("Live mode capital is capped at $10 USDC during beta.");
      return;
    }
    if (!simulate && !walletId) {
      setError("Select a wallet for live trading.");
      return;
    }
    const body: CreateSessionRequest = {
      ...(!simulate ? { wallet_id: walletId } : {}),
      copy_pct: copyPct / 100,
      max_position_usdc: maxPosition,
      max_slippage_bps: maxSlippage,
      order_type: orderType,
      initial_capital: capital,
      simulate,
      max_loss_pct: maxLossPct,
      min_source_usdc: minSourceUsdc,
      utilization_cap: utilizationCap / 100,
      max_open_positions: maxOpenPositions,
      ...(takeProfitPct !== "" ? { take_profit_pct: takeProfitPct } : {}),
      ...(stopLossPct !== "" ? { stop_loss_pct: stopLossPct } : {}),
      mirror_close: mirrorClose,
      health_interval_secs: healthInterval,
      max_source_price: maxSourcePrice / 100,
      min_source_price: minSourcePrice / 100,
      ...(source === "top_n" ? { top_n: topN } : { list_id: listId }),
    };
    create.mutate(body, {
      onSuccess: () => {
        closeModal();
        setActiveTab("sessions");
        setHeight("half");
      },
      onError: (e) => setError(e.message),
    });
  };

  return (
    <AnimatePresence>
      {open && (
        <motion.div
          ref={overlayRef}
          className="fixed inset-0 z-50 flex items-center justify-center bg-black/70 backdrop-blur-md"
          initial={{ opacity: 0 }}
          animate={{ opacity: 1 }}
          exit={{ opacity: 0 }}
          transition={{ duration: 0.15 }}
          onClick={(e) => { if (e.target === overlayRef.current) closeModal(); }}
        >
          <motion.div
            className="w-full max-w-lg mx-4 rounded-2xl border border-white/[0.08] bg-[var(--bg-panel-solid)] shadow-[0_24px_80px_-12px_rgba(0,0,0,0.6),0_0_40px_rgba(59,130,246,0.06)] overflow-hidden"
            initial={{ scale: 0.96, opacity: 0, y: 12 }}
            animate={{ scale: 1, opacity: 1, y: 0 }}
            exit={{ scale: 0.96, opacity: 0, y: 12 }}
            transition={{ type: "spring", stiffness: 400, damping: 32 }}
          >
            {/* Header gradient bar */}
            <div className="h-[2px] bg-gradient-to-r from-transparent via-[var(--accent-blue)] to-transparent opacity-60" />

            <div className="p-6 space-y-5 max-h-[80vh] overflow-y-auto">
              {/* Title */}
              <div className="flex items-center justify-between">
                <div>
                  <h2 className="text-base font-semibold text-[var(--text-primary)]">
                    New Session
                  </h2>
                  <p className="text-xs text-[var(--text-secondary)] mt-0.5">
                    Configure a copy-trade {simulate ? "simulation" : "live session"}
                  </p>
                  {prefillRef.current && (
                    <p className="text-[10px] text-[var(--accent-orange)] mt-1 truncate max-w-[320px]">
                      From {prefillRef.current.sourceSurface.replace(/_/g, " ")}
                      {prefillRef.current.question ? `: ${prefillRef.current.question.slice(0, 50)}` : prefillRef.current.traderAddress ? `: ${prefillRef.current.traderAddress.slice(0, 10)}…` : ""}
                    </p>
                  )}
                </div>
                <button
                  onClick={closeModal}
                  className="w-8 h-8 flex items-center justify-center rounded-xl text-[var(--text-secondary)] hover:text-[var(--text-primary)] hover:bg-white/[0.06] transition-all"
                >
                  <svg width="14" height="14" viewBox="0 0 14 14" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round">
                    <path d="M1 1l12 12M13 1L1 13" />
                  </svg>
                </button>
              </div>

              {/* ── Trader Source ─────────────────────────── */}
              <Section label="Trader Source">
                <Pill
                  options={[
                    { value: "top_n" as const, label: "Top N" },
                    { value: "list" as const, label: "Watchlist" },
                  ]}
                  value={source}
                  onChange={setSource}
                  color="blue"
                />
                {source === "top_n" ? (
                  <div>
                    <label className={labelCls}>Number of Traders</label>
                    <input
                      type="number"
                      className={inputCls}
                      value={topN}
                      min={1}
                      max={50}
                      onChange={(e) => setTopN(Number(e.target.value))}
                    />
                  </div>
                ) : (
                  <div>
                    <label className={labelCls}>Select Watchlist</label>
                    <select className={selectCls} value={listId} onChange={(e) => setListId(e.target.value)}>
                      <option value="">Choose a list...</option>
                      {lists?.map((l) => (
                        <option key={l.id} value={l.id}>{l.name} ({l.member_count} traders)</option>
                      ))}
                    </select>
                  </div>
                )}
              </Section>

              {/* ── Position Sizing ───────────────────────── */}
              <Section label="Position Sizing">
                <div className="grid grid-cols-2 gap-3">
                  <div>
                    <label className={labelCls}>Capital (USDC){!simulate && " — max $10"}</label>
                    <input
                      type="number"
                      className={inputCls}
                      value={capital}
                      min={1}
                      max={simulate ? undefined : 10}
                      onChange={(e) => setCapital(Number(e.target.value))}
                    />
                  </div>
                  <div>
                    <label className={labelCls}>Copy Size %</label>
                    <input
                      type="number"
                      className={inputCls}
                      value={copyPct}
                      min={5}
                      max={100}
                      onChange={(e) => setCopyPct(Number(e.target.value))}
                    />
                  </div>
                </div>
                <div className="grid grid-cols-2 gap-3">
                  <div>
                    <label className={labelCls}>Max Position (USDC)</label>
                    <input
                      type="number"
                      className={inputCls}
                      value={maxPosition}
                      min={1}
                      onChange={(e) => setMaxPosition(Number(e.target.value))}
                    />
                  </div>
                  <div>
                    <label className={labelCls}>Max Slippage (bps)</label>
                    <input
                      type="number"
                      className={inputCls}
                      value={maxSlippage}
                      min={10}
                      max={1000}
                      onChange={(e) => setMaxSlippage(Number(e.target.value))}
                    />
                  </div>
                </div>
              </Section>

              {/* ── Execution ─────────────────────────────── */}
              <Section label="Execution">
                <div className="grid grid-cols-2 gap-3">
                  <div>
                    <label className={labelCls}>Order Type</label>
                    <select className={selectCls} value={orderType} onChange={(e) => setOrderType(e.target.value as CopyOrderType)}>
                      <option value="FOK">FOK (Fill or Kill)</option>
                      <option value="GTC">GTC (Good til Canceled)</option>
                    </select>
                  </div>
                  <div>
                    <label className={labelCls}>Max Loss %</label>
                    <input
                      type="number"
                      className={inputCls}
                      value={maxLossPct}
                      min={1}
                      max={100}
                      onChange={(e) => setMaxLossPct(Number(e.target.value))}
                    />
                  </div>
                </div>
              </Section>

              {/* ── Advanced ──────────────────────────────── */}
              <div>
                <button
                  type="button"
                  className="flex items-center gap-2 text-[11px] text-[var(--text-secondary)] hover:text-[var(--text-primary)] font-medium tracking-wide transition-colors"
                  onClick={() => setShowAdvanced(!showAdvanced)}
                >
                  <svg
                    width="10"
                    height="10"
                    viewBox="0 0 10 10"
                    fill="currentColor"
                    className={`transition-transform duration-200 ${showAdvanced ? "rotate-90" : ""}`}
                  >
                    <path d="M3 1l4 4-4 4" />
                  </svg>
                  Advanced Settings
                </button>

                <AnimatePresence>
                  {showAdvanced && (
                    <motion.div
                      initial={{ height: 0, opacity: 0 }}
                      animate={{ height: "auto", opacity: 1 }}
                      exit={{ height: 0, opacity: 0 }}
                      transition={{ duration: 0.2 }}
                      className="overflow-hidden"
                    >
                      <div className="pt-3 space-y-3">
                        <div className="grid grid-cols-2 gap-3">
                          <div>
                            <label className={labelCls}>Min Source Trade ($)</label>
                            <input type="number" className={inputCls} value={minSourceUsdc} min={0} onChange={(e) => setMinSourceUsdc(Number(e.target.value))} />
                          </div>
                          <div>
                            <label className={labelCls}>Max Open Positions</label>
                            <input type="number" className={inputCls} value={maxOpenPositions} min={1} max={100} onChange={(e) => setMaxOpenPositions(Number(e.target.value))} />
                          </div>
                        </div>
                        <div className="grid grid-cols-2 gap-3">
                          <div>
                            <label className={labelCls}>Utilization Cap (%)</label>
                            <input type="number" className={inputCls} value={utilizationCap} min={10} max={100} onChange={(e) => setUtilizationCap(Number(e.target.value))} />
                          </div>
                          <div>
                            <label className={labelCls}>Health Check (sec)</label>
                            <input type="number" className={inputCls} value={healthInterval} min={10} max={300} onChange={(e) => setHealthInterval(Number(e.target.value))} />
                          </div>
                        </div>
                        <div className="grid grid-cols-2 gap-3">
                          <div>
                            <label className={labelCls}>Price Range Min (¢)</label>
                            <input type="number" className={inputCls} value={minSourcePrice} min={1} max={50} onChange={(e) => setMinSourcePrice(Number(e.target.value))} />
                          </div>
                          <div>
                            <label className={labelCls}>Price Range Max (¢)</label>
                            <input type="number" className={inputCls} value={maxSourcePrice} min={10} max={99} onChange={(e) => setMaxSourcePrice(Number(e.target.value))} />
                          </div>
                        </div>
                        <div className="grid grid-cols-2 gap-3">
                          <div>
                            <label className={labelCls}>Take Profit (%)</label>
                            <input type="number" className={inputCls} value={takeProfitPct} min={1} max={500} placeholder="Off" onChange={(e) => setTakeProfitPct(e.target.value ? Number(e.target.value) : "")} />
                          </div>
                          <div>
                            <label className={labelCls}>Stop Loss (%)</label>
                            <input type="number" className={inputCls} value={stopLossPct} min={1} max={100} placeholder="Off" onChange={(e) => setStopLossPct(e.target.value ? Number(e.target.value) : "")} />
                          </div>
                        </div>
                        <label className="flex items-center gap-2.5 cursor-pointer group">
                          <div className={`relative w-9 h-5 rounded-full transition-colors duration-200 ${mirrorClose ? "bg-[var(--accent-blue)]/40" : "bg-white/[0.08]"}`}>
                            <div className={`absolute top-0.5 w-4 h-4 rounded-full bg-white shadow-sm transition-transform duration-200 ${mirrorClose ? "translate-x-[18px]" : "translate-x-0.5"}`} />
                          </div>
                          <span className="text-xs text-[var(--text-secondary)] group-hover:text-[var(--text-primary)] transition-colors">
                            Mirror source sells
                          </span>
                        </label>
                        <input type="checkbox" className="hidden" checked={mirrorClose} onChange={(e) => setMirrorClose(e.target.checked)} />
                      </div>
                    </motion.div>
                  )}
                </AnimatePresence>
              </div>

              {/* ── Mode ──────────────────────────────────── */}
              <div className="pt-1 border-t border-white/[0.05]">
                <Section label="Mode">
                  <div className="flex items-center gap-3">
                    <Pill
                      options={[
                        { value: "sim" as const, label: "Simulate" },
                        { value: "live" as const, label: "Live" },
                      ]}
                      value={simulate ? "sim" : "live"}
                      onChange={(v) => {
                        if (v === "live" && simulate) {
                          setShowLiveConfirm(true);
                        } else {
                          setSimulate(true);
                          setShowLiveConfirm(false);
                        }
                      }}
                      color={simulate ? "green" : "red"}
                    />
                    {!simulate && (
                      <span className="text-[10px] text-red-400/80 font-mono">REAL FUNDS</span>
                    )}
                  </div>

                  {/* Live confirmation */}
                  <AnimatePresence>
                    {showLiveConfirm && simulate && (
                      <motion.div
                        initial={{ height: 0, opacity: 0 }}
                        animate={{ height: "auto", opacity: 1 }}
                        exit={{ height: 0, opacity: 0 }}
                        className="overflow-hidden"
                      >
                        <div className="rounded-xl border border-red-500/20 bg-red-500/[0.05] p-3.5 space-y-2.5">
                          <p className="text-xs text-red-300/90 leading-relaxed">
                            Live mode places real orders on Polymarket CLOB using your wallet funds. This is irreversible. Ensure your wallet is funded and CLOB credentials are derived.
                          </p>
                          <div className="flex gap-2">
                            <button
                              className="px-3 py-1.5 text-xs font-medium rounded-lg border border-red-500/30 bg-red-500/10 text-red-400 hover:bg-red-500/20 transition-colors"
                              onClick={() => {
                                setSimulate(false);
                                setShowLiveConfirm(false);
                                if (capital > 10) setCapital(10);
                              }}
                            >
                              Confirm Live Mode
                            </button>
                            <button
                              className="px-3 py-1.5 text-xs rounded-lg text-[var(--text-secondary)] hover:text-[var(--text-primary)] transition-colors"
                              onClick={() => setShowLiveConfirm(false)}
                            >
                              Cancel
                            </button>
                          </div>
                        </div>
                      </motion.div>
                    )}
                  </AnimatePresence>

                  {/* Wallet selector (live only) */}
                  {!simulate && (
                    <div>
                      <label className={labelCls}>Wallet</label>
                      <select className={selectCls} value={walletId} onChange={(e) => setWalletId(e.target.value)}>
                        <option value="">Choose credentialed wallet...</option>
                        {liveWallets.map((w) => (
                          <option key={w.id} value={w.id}>{w.address.slice(0, 8)}... ({w.status})</option>
                        ))}
                      </select>
                      {liveWallets.length === 0 && (
                        <p className="text-[10px] text-red-400/80 mt-1.5">
                          No credentialed wallet available. Derive credentials in Wallet tab first.
                        </p>
                      )}
                    </div>
                  )}
                </Section>
              </div>

              {/* ── Error ─────────────────────────────────── */}
              {error && (
                <div className="rounded-xl bg-red-500/[0.08] border border-red-500/20 px-3.5 py-2.5 text-xs text-red-400">
                  {error}
                </div>
              )}

              {/* ── Submit ────────────────────────────────── */}
              <button
                type="button"
                className={`w-full py-3 text-sm font-semibold rounded-xl border transition-all duration-200 disabled:opacity-40 disabled:cursor-not-allowed ${
                  simulate
                    ? "bg-gradient-to-r from-[var(--accent-blue)]/20 to-[var(--accent-blue)]/10 text-[var(--accent-blue)] border-[var(--accent-blue)]/20 hover:from-[var(--accent-blue)]/30 hover:to-[var(--accent-blue)]/15 hover:shadow-[0_0_20px_rgba(59,130,246,0.12)]"
                    : "bg-gradient-to-r from-red-500/20 to-red-500/10 text-red-400 border-red-500/20 hover:from-red-500/30 hover:to-red-500/15 hover:shadow-[0_0_20px_rgba(239,68,68,0.12)]"
                }`}
                onClick={handleSubmit}
                disabled={create.isPending || (source === "list" && !listId) || (!simulate && !walletId)}
              >
                {create.isPending
                  ? "Starting..."
                  : simulate
                    ? "Start Simulation"
                    : "Start Live Trading"}
              </button>
            </div>
          </motion.div>
        </motion.div>
      )}
    </AnimatePresence>
  );
}
