import type { ReactNode } from "react";
import { Link, useLocation } from "react-router-dom";
import { useQuery } from "@tanstack/react-query";
import { motion } from "motion/react";
import { fetchHealth } from "../api";
import { formatNumber } from "../lib/format";
import ConnectWallet from "./ConnectWallet";
import { TerminalShell } from "./Terminal";

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
      <motion.header
        initial={{ opacity: 0, y: -10 }}
        animate={{ opacity: 1, y: 0 }}
        transition={{ duration: 0.4 }}
        className="sticky top-0 z-10 glass gradient-border-bottom"
        style={{ borderRadius: 0 }}
      >
        <div className="max-w-7xl mx-auto px-6 py-4 flex items-center justify-between">
          <Link to="/" className="flex items-center gap-3 group">
            <motion.div
              whileHover={{ rotate: [0, -5, 5, 0] }}
              transition={{ duration: 0.4 }}
              className="w-8 h-8 rounded-lg bg-gradient-to-br from-blue-500 to-orange-500 flex items-center justify-center text-xs font-black text-white shadow-lg shadow-blue-500/20"
            >
              PD
            </motion.div>
            <span className="text-lg font-bold gradient-text tracking-tight">
              PolyDerboard
            </span>
          </Link>
          <nav className="flex items-center gap-6 text-sm">
            {[
              { to: "/", label: "Leaderboard" },
              { to: "/activity", label: "Activity" },
              { to: "/alerts", label: "Alerts" },
              { to: "/lab", label: "Lab" },
            ].map((link) => (
              <motion.div key={link.to} whileHover={{ y: -1 }} transition={{ duration: 0.15 }}>
                <Link
                  to={link.to}
                  className={`transition-colors duration-200 ${
                    pathname === link.to
                      ? "text-[var(--accent-blue)] font-semibold"
                      : "text-[var(--text-secondary)] hover:text-[var(--accent-blue)]"
                  }`}
                >
                  {link.label}
                </Link>
              </motion.div>
            ))}
            <ConnectWallet />
          </nav>
        </div>
      </motion.header>

      {/* Main */}
      <main className="flex-1 max-w-7xl mx-auto px-6 py-8 w-full" style={{ paddingBottom: "var(--terminal-height, 40px)" }}>{children}</main>

      {/* Footer */}
      <footer className="glass gradient-border-top" style={{ borderRadius: 0 }}>
        <div className="max-w-7xl mx-auto px-6 py-3 flex items-center justify-between text-xs">
          <span className="text-[var(--text-secondary)]">Polymarket On-Chain Leaderboard</span>
          {health && (
            <div className="flex items-center gap-5 text-[var(--text-secondary)]">
              <span className="flex items-center gap-1.5">
                <span className="w-1 h-1 rounded-full bg-blue-400/60" />
                {formatNumber(health.trade_count)} trades
              </span>
              <span className="flex items-center gap-1.5">
                <span className="w-1 h-1 rounded-full bg-orange-400/60" />
                {formatNumber(health.trader_count)} traders
              </span>
              <span className="flex items-center gap-1.5">
                <span className="w-1.5 h-1.5 rounded-full bg-[var(--neon-green)] animate-pulse shadow-[0_0_6px_var(--neon-green)]" />
                <span className="glow-blue font-mono">#{formatNumber(health.latest_block)}</span>
              </span>
            </div>
          )}
        </div>
      </footer>
      {/* Terminal */}
      <TerminalShell />
    </div>
  );
}
