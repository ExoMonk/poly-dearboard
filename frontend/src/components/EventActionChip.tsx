import { motion } from "motion/react";
import { tapScale } from "../lib/motion";

export type ActionKind =
  | "open_trader"
  | "open_market"
  | "open_tx"
  | "copy_trade"
  | "add_trader";

const CONFIG: Record<ActionKind, { label: string; icon: string; cls: string }> = {
  open_trader:  { label: "Open trader",      icon: "↗", cls: "text-[var(--accent-blue)]" },
  open_market:  { label: "Open market",      icon: "◉", cls: "text-[var(--accent-orange)]" },
  open_tx:      { label: "Open tx",          icon: "⧉", cls: "text-[var(--text-secondary)]" },
  copy_trade:   { label: "Start copy-trade", icon: "⚡", cls: "text-[var(--neon-green)]" },
  add_trader:   { label: "Add trader",       icon: "+", cls: "text-[var(--accent-blue)]" },
};

interface Props {
  kind: ActionKind;
  onClick: (e: React.MouseEvent) => void;
}

export default function EventActionChip({ kind, onClick }: Props) {
  const c = CONFIG[kind];
  return (
    <motion.button
      whileTap={tapScale}
      onClick={(e) => { e.stopPropagation(); e.preventDefault(); onClick(e); }}
      className={`inline-flex items-center gap-1 px-2 py-1 text-[10px] font-medium rounded-md
        bg-white/[0.04] border border-white/[0.06] hover:bg-white/[0.08] hover:border-white/[0.12]
        transition-all duration-150 cursor-pointer whitespace-nowrap ${c.cls}`}
      title={c.label}
    >
      <span className="text-[11px] leading-none">{c.icon}</span>
      <span className="hidden sm:inline">{c.label}</span>
    </motion.button>
  );
}

/** Normalized label lookup for mobile dropdown */
export function actionLabel(kind: ActionKind): string {
  return CONFIG[kind].label;
}
