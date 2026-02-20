import { useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { motion } from "motion/react";
import { Link } from "react-router-dom";
import { fetchSmartMoney } from "../api";
import { formatUsd } from "../lib/format";
import { panelVariants } from "../lib/motion";
import type { Timeframe } from "../types";

const TOP_OPTIONS = [10, 25, 50] as const;

interface Props {
  timeframe?: Timeframe;
}

export default function SmartMoney({ timeframe }: Props) {
  const [top, setTop] = useState<number>(10);

  const { data, isLoading } = useQuery({
    queryKey: ["smart-money", timeframe, top],
    queryFn: () => fetchSmartMoney({ timeframe, top }),
    refetchInterval: 60_000,
  });

  const markets = data?.markets ?? [];

  return (
    <motion.div
      variants={panelVariants}
      initial="initial"
      animate="animate"
      transition={{ duration: 0.4 }}
      className="glass p-5 gradient-border-top"
    >
      <div className="flex items-center justify-between mb-4">
        <div>
          <h3 className="text-sm font-medium text-[var(--text-secondary)] uppercase tracking-wider">
            Smart Money
          </h3>
          <p className="text-xs text-[var(--text-secondary)] opacity-60 mt-0.5">
            Where top {top} PnL traders are positioned
          </p>
        </div>
        <div className="flex gap-1">
          {TOP_OPTIONS.map((n) => (
            <button
              key={n}
              onClick={() => setTop(n)}
              className={`text-[10px] px-2 py-0.5 rounded-full transition-colors ${
                top === n
                  ? "bg-[var(--accent-blue)]/20 text-[var(--accent-blue)]"
                  : "text-[var(--text-secondary)] hover:text-[var(--text-primary)] hover:bg-white/5"
              }`}
            >
              Top {n}
            </button>
          ))}
        </div>
      </div>

      {isLoading ? (
        <div className="flex items-center justify-center py-12">
          <div className="w-5 h-5 border-2 border-[var(--accent-blue)] border-t-transparent rounded-full animate-spin" />
        </div>
      ) : markets.length === 0 ? (
        <div className="text-center py-12 text-[var(--text-secondary)] text-sm">
          No active smart money positions
        </div>
      ) : (
        <div className="space-y-2">
          {markets.map((m, i) => {
            const longExp = parseFloat(m.long_exposure) || 0;
            const shortExp = parseFloat(m.short_exposure) || 0;
            const total = longExp + shortExp;
            const longPct = total > 0 ? (longExp / total) * 100 : 50;
            const shortPct = total > 0 ? (shortExp / total) * 100 : 50;
            const sentiment = longExp > shortExp ? "Bullish" : shortExp > longExp ? "Bearish" : "Split";

            return (
              <motion.div
                key={m.token_id}
                initial={{ opacity: 0, x: -8 }}
                animate={{ opacity: 1, x: 0 }}
                transition={{ duration: 0.25, delay: i * 0.04 }}
                className="flex items-center gap-3 px-3 py-2.5 rounded-lg hover:bg-white/[0.02] transition-colors group"
              >
                {/* Rank */}
                <span className="text-xs font-mono text-[var(--text-secondary)] w-5 shrink-0">
                  {i + 1}
                </span>

                {/* Market info */}
                <div className="flex-1 min-w-0">
                  <Link
                    to={`/market/${m.token_id}`}
                    className="text-sm text-[var(--text-primary)] hover:text-[var(--accent-cyan)] transition-colors truncate block"
                    title={m.question}
                  >
                    {m.question}
                  </Link>

                  {/* Exposure bar */}
                  <div className="flex items-center gap-2 mt-1.5">
                    <div className="flex-1 h-1.5 rounded-full bg-[var(--surface-primary)] overflow-hidden flex">
                      <div
                        className="h-full rounded-l-full"
                        style={{
                          width: `${longPct}%`,
                          background: "linear-gradient(90deg, rgba(74,222,128,0.4), rgba(74,222,128,0.7))",
                        }}
                      />
                      <div
                        className="h-full rounded-r-full"
                        style={{
                          width: `${shortPct}%`,
                          background: "linear-gradient(90deg, rgba(248,113,113,0.7), rgba(248,113,113,0.4))",
                        }}
                      />
                    </div>
                  </div>

                  {/* Labels */}
                  <div className="flex items-center justify-between mt-1 text-[10px]">
                    <span className="text-green-400/70">
                      {m.long_count}L {formatUsd(m.long_exposure)}
                    </span>
                    <span className="text-red-400/70">
                      {m.short_count}S {formatUsd(m.short_exposure)}
                    </span>
                  </div>
                </div>

                {/* Right side: sentiment + trader count */}
                <div className="flex flex-col items-end gap-1 shrink-0">
                  <span
                    className={`text-[10px] font-semibold px-1.5 py-0.5 rounded ${
                      sentiment === "Bullish"
                        ? "text-green-400 bg-green-400/10"
                        : sentiment === "Bearish"
                          ? "text-red-400 bg-red-400/10"
                          : "text-[var(--text-secondary)] bg-white/5"
                    }`}
                  >
                    {sentiment}
                  </span>
                  <span className="text-[10px] text-[var(--text-secondary)] font-mono">
                    {m.smart_trader_count}/{top}
                  </span>
                </div>
              </motion.div>
            );
          })}
        </div>
      )}
    </motion.div>
  );
}
