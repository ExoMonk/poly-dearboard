import type { BehavioralLabel } from "../types";

const LABEL_STYLES: Record<BehavioralLabel, { text: string; bg: string; border: string; glow: string }> = {
  sharp: { text: "Sharp", bg: "bg-emerald-500/10", border: "border-emerald-500/30", glow: "shadow-[0_0_8px_rgba(16,185,129,0.2)]" },
  specialist: { text: "Specialist", bg: "bg-cyan-500/10", border: "border-cyan-500/30", glow: "shadow-[0_0_8px_rgba(6,182,212,0.2)]" },
  whale: { text: "Whale", bg: "bg-blue-500/10", border: "border-blue-500/30", glow: "shadow-[0_0_8px_rgba(59,130,246,0.2)]" },
  degen: { text: "Degen", bg: "bg-orange-500/10", border: "border-orange-500/30", glow: "shadow-[0_0_8px_rgba(249,115,22,0.2)]" },
  market_maker: { text: "Market Maker", bg: "bg-purple-500/10", border: "border-purple-500/30", glow: "shadow-[0_0_8px_rgba(168,85,247,0.2)]" },
  bot: { text: "Bot", bg: "bg-yellow-500/10", border: "border-yellow-500/30", glow: "shadow-[0_0_8px_rgba(234,179,8,0.2)]" },
  casual: { text: "Casual", bg: "bg-gray-500/10", border: "border-gray-500/30", glow: "" },
  contrarian: { text: "Contrarian", bg: "bg-rose-500/10", border: "border-rose-500/30", glow: "shadow-[0_0_8px_rgba(244,63,94,0.2)]" },
};

export { LABEL_STYLES };

export default function LabelBadge({ label, size = "md", tooltip }: {
  label: BehavioralLabel;
  size?: "sm" | "md";
  tooltip?: string;
}) {
  const style = LABEL_STYLES[label];
  const sizeClass = size === "sm" ? "text-[10px] px-2 py-0.5" : "text-xs px-3 py-1";
  return (
    <span className={`relative group inline-block ${tooltip ? "cursor-help" : ""}`}>
      <span
        className={`font-bold rounded-full border ${sizeClass} ${style.bg} ${style.border} ${style.glow}`}
      >
        {style.text}
      </span>
      {tooltip && (
        <span className="pointer-events-none absolute bottom-full left-1/2 -translate-x-1/2 mb-2 px-3 py-1.5 rounded-lg text-xs text-[var(--text-primary)] bg-[var(--bg-panel-solid)] border border-[var(--border-glow)] shadow-[0_0_12px_rgba(59,130,246,0.15)] whitespace-nowrap opacity-0 group-hover:opacity-100 transition-opacity duration-150 z-50">
          {tooltip}
          <span className="absolute top-full left-1/2 -translate-x-1/2 border-4 border-transparent border-t-[var(--bg-panel-solid)]" />
        </span>
      )}
    </span>
  );
}
