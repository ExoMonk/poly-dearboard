import { useState } from "react";
import { Link, useNavigate } from "react-router-dom";
import { motion, AnimatePresence } from "motion/react";
import useAlerts from "../hooks/useAlerts";
import { useAuth } from "../context/AuthContext";
import type { Alert } from "../types";
import { formatUsd, formatNumber, shortenAddress, polygonscanTx, polygonscanAddress, timeAgo } from "../lib/format";
import { alertCardVariants, tapScale } from "../lib/motion";
import EventActions, { type ActionDef } from "../components/EventActions";
import AddToListButton from "../components/AddToListButton";
import { requestOpenCreateSession } from "../components/Terminal/CreateSessionModal";

type AlertFilter = "all" | "whale" | "resolution" | "failed";

const FILTERS: { value: AlertFilter; label: string }[] = [
  { value: "all", label: "All" },
  { value: "whale", label: "Whale Trades" },
  { value: "resolution", label: "Resolutions" },
  { value: "failed", label: "Failed TXs" },
];

export default function Alerts() {
  const { alerts, connected } = useAlerts();
  const [filter, setFilter] = useState<AlertFilter>("all");

  const filtered = alerts.filter((a) => {
    if (filter === "whale") return a.kind === "WhaleTrade";
    if (filter === "resolution") return a.kind === "MarketResolution";
    if (filter === "failed") return a.kind === "FailedSettlement";
    return true;
  });

  // Count by type for badges
  const counts = {
    whale: alerts.filter((a) => a.kind === "WhaleTrade").length,
    resolution: alerts.filter((a) => a.kind === "MarketResolution").length,
    failed: alerts.filter((a) => a.kind === "FailedSettlement").length,
  };

  return (
    <div className="space-y-8">
      <div className="flex items-center justify-between">
        <div className="flex items-center gap-3">
          <h1 className="text-3xl font-black gradient-text tracking-tight glitch-text">Alerts</h1>
          <span
            className={`flex items-center gap-1.5 text-xs px-2.5 py-1 rounded-full border ${
              connected
                ? "text-[var(--neon-green)] border-[var(--neon-green)]/30 bg-[var(--neon-green)]/5"
                : "text-[var(--neon-red)] border-[var(--neon-red)]/30 bg-[var(--neon-red)]/5"
            }`}
          >
            <span
              className={`w-1.5 h-1.5 rounded-full ${
                connected
                  ? "bg-[var(--neon-green)] neon-pulse shadow-[0_0_6px_var(--neon-green)]"
                  : "bg-[var(--neon-red)]"
              }`}
            />
            {connected ? "Live" : "Reconnecting"}
          </span>
          {alerts.length > 0 && (
            <span className="text-xs text-[var(--text-secondary)] font-mono">
              {alerts.length} alerts
            </span>
          )}
        </div>
        <div className="flex gap-1">
          {FILTERS.map((f) => {
            const count = f.value === "all" ? 0 : counts[f.value as keyof typeof counts] || 0;
            return (
              <motion.button
                key={f.value}
                onClick={() => setFilter(f.value)}
                whileTap={tapScale}
                className={`px-4 py-1.5 text-xs rounded-full font-medium transition-all duration-200 flex items-center gap-1.5 ${
                  filter === f.value
                    ? f.value === "failed"
                      ? "bg-[var(--neon-red)]/10 text-[var(--neon-red)] border border-[var(--neon-red)]/30 shadow-[0_0_8px_rgba(255,51,102,0.15)]"
                      : "bg-[var(--accent-blue)]/10 text-[var(--accent-blue)] border border-[var(--accent-blue)]/30 shadow-[0_0_8px_rgba(59,130,246,0.15)]"
                    : "text-[var(--text-secondary)] border border-transparent hover:text-[var(--text-primary)] hover:border-[var(--border-glow)]"
                }`}
              >
                {f.label}
                {count > 0 && (
                  <span className={`text-[10px] font-bold px-1.5 py-0.5 rounded-full ${
                    f.value === "failed"
                      ? "bg-[var(--neon-red)]/20 text-[var(--neon-red)]"
                      : "bg-[var(--accent-blue)]/20 text-[var(--accent-blue)]"
                  }`}>
                    {count}
                  </span>
                )}
              </motion.button>
            );
          })}
        </div>
      </div>

      {filtered.length === 0 ? (
        <div className="glass p-16 text-center">
          <div className="text-[var(--text-secondary)] text-sm">
            {alerts.length === 0
              ? "Listening for alerts..."
              : "No alerts match this filter"}
          </div>
          <div className="text-[var(--text-secondary)]/50 text-xs mt-2">
            Whale trades, market resolutions, and failed settlements appear here in real time
          </div>
        </div>
      ) : (
        <div className="space-y-3">
          <AnimatePresence initial={false}>
            {filtered.map((alert, i) => (
              <motion.div
                key={`${alert.tx_hash}-${i}`}
                variants={alertCardVariants}
                initial="initial"
                animate="animate"
                exit="exit"
                layout
              >
                <AlertCard alert={alert} />
              </motion.div>
            ))}
          </AnimatePresence>
        </div>
      )}
    </div>
  );
}

function AlertCard({ alert }: { alert: Alert }) {
  if (alert.kind === "WhaleTrade") {
    return <WhaleTradeCard alert={alert} />;
  }
  if (alert.kind === "FailedSettlement") {
    return <FailedSettlementCard alert={alert} />;
  }
  return <MarketResolutionCard alert={alert} />;
}

// ---------------------------------------------------------------------------
// Whale Trade Card
// ---------------------------------------------------------------------------

function WhaleTradeCard({ alert }: { alert: Extract<Alert, { kind: "WhaleTrade" }> }) {
  const isBuy = alert.side === "buy";
  const navigate = useNavigate();
  const { isAuthenticated, signIn } = useAuth();

  const actions: ActionDef[] = [
    { kind: "add_trader", onClick: () => {}, render: <AddToListButton address={alert.trader} /> },
    {
      kind: "copy_trade",
      onClick: () => {
        if (!isAuthenticated) { signIn(); return; }
        requestOpenCreateSession({
          sourceSurface: "alerts",
          sourceKind: "whale_trade",
          traderAddress: alert.trader,
          tokenId: alert.asset_id,
          question: alert.question,
          outcome: alert.outcome,
          defaults: { simulationMode: "simulate" },
        });
      },
    },
    { kind: "open_trader", onClick: () => navigate(`/trader/${alert.trader}`) },
    { kind: "open_market", onClick: () => navigate(`/market/${encodeURIComponent(alert.asset_id)}`) },
  ];

  return (
    <div className={`glass p-5 transition-all duration-300 hover:shadow-lg hover:shadow-[var(--accent-blue)]/5 group ${
      isBuy
        ? "border-l-4 border-[var(--neon-green)]/60"
        : "border-l-4 border-[var(--neon-red)]/60"
    }`}>
      <div className="flex items-start justify-between gap-4">
        <div className="flex items-center gap-3 min-w-0">
          <span
            className={`text-xs font-bold px-3 py-1.5 rounded-full shrink-0 ${
              isBuy
                ? "bg-[var(--neon-green)]/10 text-[var(--neon-green)] shadow-[0_0_8px_rgba(0,255,136,0.2)]"
                : "bg-[var(--neon-red)]/10 text-[var(--neon-red)] shadow-[0_0_8px_rgba(255,51,102,0.2)]"
            }`}
          >
            {isBuy ? "BUY" : "SELL"}
          </span>
          <div className="min-w-0">
            <div className="flex items-center gap-2">
              <span className={`font-mono font-black text-xl tracking-tight ${isBuy ? "glow-green" : "glow-red"}`}>
                {formatUsd(alert.usdc_amount)}
              </span>
              <span className="text-[var(--text-secondary)] text-xs px-2 py-0.5 rounded bg-[var(--bg-card)]/50">
                {alert.exchange === "neg_risk" ? "NegRisk" : "CTF"}
              </span>
            </div>
            {alert.question ? (
              <Link
                to={`/market/${encodeURIComponent(alert.asset_id)}`}
                className="text-sm text-[var(--text-secondary)] hover:text-[var(--accent-blue)] truncate mt-1.5 block transition-colors duration-200"
                title={alert.question}
              >
                {alert.question}
                {alert.outcome && (
                  <span className="text-[var(--accent-orange)] ml-1 font-medium">({alert.outcome})</span>
                )}
              </Link>
            ) : (
              <div className="text-xs text-[var(--text-secondary)]/50 mt-1.5 font-mono">
                {shortenAddress(alert.asset_id)}
              </div>
            )}
          </div>
        </div>
        <div className="text-right shrink-0 text-xs space-y-1.5">
          <Link
            to={`/trader/${alert.trader}`}
            className="text-[var(--accent-blue)]/60 hover:text-[var(--accent-blue)] font-mono transition-colors duration-200 block"
          >
            {shortenAddress(alert.trader)}
          </Link>
          <a
            href={polygonscanTx(alert.tx_hash)}
            target="_blank"
            rel="noopener noreferrer"
            className="text-[var(--text-secondary)]/50 hover:text-[var(--accent-blue)] font-mono transition-colors duration-200 block"
          >
            tx {shortenAddress(alert.tx_hash)}
          </a>
          {alert.timestamp && (
            <div className="text-[var(--text-secondary)]/40">{timeAgo(alert.timestamp)}</div>
          )}
        </div>
      </div>
      <div className="flex items-center justify-end gap-1 mt-3 pt-2 border-t border-white/[0.04]">
        <EventActions actions={actions} />
      </div>
    </div>
  );
}

// ---------------------------------------------------------------------------
// Market Resolution Card
// ---------------------------------------------------------------------------

function MarketResolutionCard({ alert }: { alert: Extract<Alert, { kind: "MarketResolution" }> }) {
  const navigate = useNavigate();

  const actions: ActionDef[] = [];
  if (alert.token_id) {
    actions.push({ kind: "open_market", onClick: () => navigate(`/market/${alert.token_id}`) });
  }

  // Default to Yes/No for binary markets when backend cache missed the outcome names
  const effectiveOutcomes = alert.outcomes.length > 0
    ? alert.outcomes
    : alert.payout_numerators.length === 2
      ? ["Yes", "No"]
      : alert.payout_numerators.map((_, i) => `Outcome ${i + 1}`);

  const winningOutcome = alert.winning_outcome || deriveWinner(alert.payout_numerators, effectiveOutcomes);
  const isYesWin = winningOutcome?.toLowerCase() === "yes";

  return (
    <div className={`glass p-5 border-l-4 border-[var(--accent-orange)]/60 transition-all duration-300 hover:shadow-lg hover:shadow-[var(--accent-orange)]/5`}>
      <div className="flex items-start justify-between gap-4">
        <div className="flex items-center gap-3 min-w-0">
          <div className="flex flex-col items-center gap-1 shrink-0">
            <span className="text-xs font-bold px-3 py-1.5 rounded-full bg-[var(--accent-orange)]/10 text-[var(--accent-orange)] border border-[var(--accent-orange)]/20 shadow-[0_0_8px_rgba(249,115,22,0.15)]">
              RESOLVED
            </span>
            {winningOutcome && (
              <span className={`text-[10px] font-bold px-2 py-0.5 rounded-full ${
                isYesWin
                  ? "bg-[var(--neon-green)]/10 text-[var(--neon-green)]"
                  : "bg-[var(--neon-red)]/10 text-[var(--neon-red)]"
              }`}>
                {winningOutcome} Won
              </span>
            )}
          </div>
          <div className="min-w-0">
            <div className="text-sm font-medium text-[var(--text-primary)]">
              {alert.question ? (
                alert.token_id ? (
                  <Link to={`/market/${alert.token_id}`} className="hover:text-[var(--accent-blue)] transition-colors duration-200">
                    {alert.question}
                  </Link>
                ) : (
                  alert.question
                )
              ) : (
                "Market Resolved"
              )}
            </div>
            <div className="flex items-center gap-3 mt-1.5 flex-wrap">
              <div className="flex gap-1.5">
                {effectiveOutcomes.map((outcome, i) => {
                  const isWinner = alert.payout_numerators[i] && parseInt(alert.payout_numerators[i]) > 0;
                  return (
                    <span
                      key={i}
                      className={`text-[10px] px-2 py-0.5 rounded font-mono ${
                        isWinner
                          ? "bg-[var(--neon-green)]/15 text-[var(--neon-green)] font-bold"
                          : "bg-[var(--bg-card)]/50 text-[var(--text-secondary)]/50 line-through"
                      }`}
                    >
                      {outcome}: {alert.payout_numerators[i] || "0"}
                    </span>
                  );
                })}
              </div>
              <a
                href={`https://polygonscan.com/tx/${alert.tx_hash}#eventlog`}
                target="_blank"
                rel="noopener noreferrer"
                className="text-[10px] text-[var(--text-secondary)]/40 hover:text-[var(--accent-blue)] font-mono transition-colors duration-200"
              >
                {shortenAddress(alert.condition_id)}
              </a>
            </div>
          </div>
        </div>
        <div className="text-right shrink-0 text-xs space-y-1.5">
          <a
            href={polygonscanTx(alert.tx_hash)}
            target="_blank"
            rel="noopener noreferrer"
            className="text-[var(--text-secondary)]/50 hover:text-[var(--accent-blue)] font-mono transition-colors duration-200 block"
          >
            tx {shortenAddress(alert.tx_hash)}
          </a>
          {alert.timestamp && (
            <div className="text-[var(--text-secondary)]/40">{timeAgo(alert.timestamp)}</div>
          )}
        </div>
      </div>
      {actions.length > 0 && (
        <div className="flex items-center justify-end gap-1 mt-3 pt-2 border-t border-white/[0.04]">
          <EventActions actions={actions} />
        </div>
      )}
    </div>
  );
}

// ---------------------------------------------------------------------------
// Failed Settlement Card (Phantom Fill)
// ---------------------------------------------------------------------------

function FailedSettlementCard({ alert }: { alert: Extract<Alert, { kind: "FailedSettlement" }> }) {
  const contractLabel = alert.to_contract === "neg_risk" ? "NegRisk Exchange" : "CTF Exchange";
  const navigate = useNavigate();

  const actions: ActionDef[] = [
    { kind: "open_tx", onClick: () => window.open(polygonscanTx(alert.tx_hash), "_blank") },
  ];
  if (alert.from_address) {
    actions.push({ kind: "open_trader", onClick: () => navigate(`/trader/${alert.from_address}`) });
  }

  return (
    <div className={`glass p-5 border-l-4 border-[var(--neon-red)] transition-all duration-300 hover:shadow-lg hover:shadow-[var(--neon-red)]/10`}>
      <div className="flex items-start justify-between gap-4">
        <div className="flex items-center gap-3 min-w-0">
          <span className="text-xs font-bold px-3 py-1.5 rounded-full shrink-0 bg-[var(--neon-red)]/15 text-[var(--neon-red)] border border-[var(--neon-red)]/30 shadow-[0_0_10px_rgba(255,51,102,0.25)] animate-pulse">
            FAILED TX
          </span>
          <div className="min-w-0">
            <div className="flex items-center gap-2">
              <span className="font-mono font-bold text-sm text-[var(--neon-red)]">
                Reverted Settlement
              </span>
              <span className="text-[var(--text-secondary)] text-xs px-2 py-0.5 rounded bg-[var(--bg-card)]/50">
                {contractLabel}
              </span>
            </div>
            <div className="flex items-center gap-3 mt-1.5 text-xs text-[var(--text-secondary)]">
              <span>
                fn: <code className="text-[var(--accent-blue)] font-bold">{alert.function_name}</code>
              </span>
              <span className="text-[var(--text-secondary)]/50">|</span>
              <span>gas: <span className="font-mono">{formatNumber(alert.gas_used)}</span></span>
            </div>
          </div>
        </div>
        <div className="text-right shrink-0 text-xs space-y-1.5">
          <a
            href={polygonscanAddress(alert.from_address)}
            target="_blank"
            rel="noopener noreferrer"
            className="text-[var(--neon-red)]/60 hover:text-[var(--neon-red)] font-mono transition-colors duration-200 block"
          >
            {shortenAddress(alert.from_address)}
          </a>
          <a
            href={polygonscanTx(alert.tx_hash)}
            target="_blank"
            rel="noopener noreferrer"
            className="text-[var(--text-secondary)]/50 hover:text-[var(--accent-blue)] font-mono transition-colors duration-200 block"
          >
            tx {shortenAddress(alert.tx_hash)}
          </a>
          <div className="text-[var(--text-secondary)]/40">
            block {formatNumber(alert.block_number)}
          </div>
        </div>
      </div>
      <div className="flex items-center justify-end gap-1 mt-3 pt-2 border-t border-white/[0.04]">
        <EventActions actions={actions} />
      </div>
    </div>
  );
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

function deriveWinner(numerators: string[], outcomes: string[]): string | undefined {
  if (outcomes.length === 0) {
    // No outcome names — try to return "Outcome N" based on payout index
    const idx = numerators.findIndex((n) => parseInt(n) > 0);
    return idx >= 0 ? `Outcome ${idx + 1}` : undefined;
  }
  const idx = numerators.findIndex((n) => parseInt(n) > 0);
  return idx >= 0 ? outcomes[idx] : undefined;
}
