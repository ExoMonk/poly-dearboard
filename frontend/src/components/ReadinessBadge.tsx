import type { TraderReadiness } from "../types";
import { BUCKET_CONFIG, readinessTooltip } from "../lib/readiness";

export default function ReadinessBadge({
  readiness,
  size = "md",
}: {
  readiness: TraderReadiness;
  size?: "sm" | "md";
}) {
  const bucket = BUCKET_CONFIG[readiness.bucket];
  const sizeClass =
    size === "sm" ? "text-[10px] px-2 py-0.5" : "text-xs px-3 py-1";
  const tooltip = readinessTooltip(readiness);

  return (
    <span className="relative group inline-block cursor-help">
      <span
        className={`font-bold rounded-full border ${sizeClass} ${bucket.bg} ${bucket.border} ${bucket.color}`}
      >
        {readiness.score}
      </span>
      <span className="pointer-events-none absolute bottom-full left-1/2 -translate-x-1/2 mb-2 px-3 py-1.5 rounded-lg text-xs text-[var(--text-primary)] bg-[var(--bg-panel-solid)] border border-[var(--border-glow)] shadow-[0_0_12px_rgba(59,130,246,0.15)] whitespace-nowrap opacity-0 group-hover:opacity-100 transition-opacity duration-150 z-50">
        {tooltip}
        <span className="absolute top-full left-1/2 -translate-x-1/2 border-4 border-transparent border-t-[var(--bg-panel-solid)]" />
      </span>
    </span>
  );
}
