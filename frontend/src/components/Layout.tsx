import type { ReactNode } from "react";
import { Link, useLocation } from "react-router-dom";
import { useQuery } from "@tanstack/react-query";
import { fetchHealth } from "../api";
import { formatNumber } from "../lib/format";

export default function Layout({ children }: { children: ReactNode }) {
  const { pathname } = useLocation();
  const { data: health } = useQuery({
    queryKey: ["health"],
    queryFn: fetchHealth,
    refetchInterval: 30_000,
  });

  return (
    <div className="min-h-screen flex flex-col">
      {/* Header */}
      <header className="sticky top-0 z-10 glass gradient-border-bottom shimmer-border" style={{ borderRadius: 0 }}>
        <div className="max-w-7xl mx-auto px-6 py-4 flex items-center justify-between">
          <Link to="/" className="flex items-center gap-3 group">
            <div className="w-8 h-8 rounded-lg bg-gradient-to-br from-cyan-400 to-purple-500 flex items-center justify-center text-xs font-black text-white shadow-lg shadow-cyan-500/20">
              PD
            </div>
            <span className="text-lg font-bold gradient-text tracking-tight">
              Poly Dearboard
            </span>
          </Link>
          <nav className="flex items-center gap-6 text-sm">
            <Link to="/" className={`transition-colors duration-200 ${pathname === "/" ? "text-[var(--accent-cyan)] font-semibold" : "text-[var(--text-secondary)] hover:text-[var(--accent-cyan)]"}`}>
              Leaderboard
            </Link>
            <Link to="/activity" className={`transition-colors duration-200 ${pathname === "/activity" ? "text-[var(--accent-cyan)] font-semibold" : "text-[var(--text-secondary)] hover:text-[var(--accent-cyan)]"}`}>
              Activity
            </Link>
          </nav>
        </div>
      </header>

      {/* Main */}
      <main className="flex-1 max-w-7xl mx-auto px-6 py-8 w-full">{children}</main>

      {/* Footer */}
      <footer className="glass gradient-border-top" style={{ borderRadius: 0 }}>
        <div className="max-w-7xl mx-auto px-6 py-3 flex items-center justify-between text-xs">
          <span className="text-[var(--text-secondary)]">Polymarket On-Chain Leaderboard</span>
          {health && (
            <div className="flex items-center gap-5 text-[var(--text-secondary)]">
              <span className="flex items-center gap-1.5">
                <span className="w-1 h-1 rounded-full bg-cyan-400/60" />
                {formatNumber(health.trade_count)} trades
              </span>
              <span className="flex items-center gap-1.5">
                <span className="w-1 h-1 rounded-full bg-purple-400/60" />
                {formatNumber(health.trader_count)} traders
              </span>
              <span className="flex items-center gap-1.5">
                <span className="w-1.5 h-1.5 rounded-full bg-[var(--neon-green)] animate-pulse shadow-[0_0_6px_var(--neon-green)]" />
                <span className="glow-cyan font-mono">#{formatNumber(health.latest_block)}</span>
              </span>
            </div>
          )}
        </div>
      </footer>
    </div>
  );
}
