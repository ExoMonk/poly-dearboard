import type { DiscoveryCategory } from "../types";
import { CATEGORY_CONFIG } from "../lib/readiness";

export default function CategoryChip({
  category,
  size = "sm",
  onClick,
  active,
}: {
  category: DiscoveryCategory;
  size?: "sm" | "md";
  onClick?: () => void;
  active?: boolean;
}) {
  const cfg = CATEGORY_CONFIG[category];
  const sizeClass =
    size === "sm" ? "text-[10px] px-2 py-0.5" : "text-xs px-2.5 py-0.5";
  const activeBg = active
    ? cfg.bg.replace("/10", "/30") + " " + cfg.border.replace("/30", "/60") + " text-white"
    : `${cfg.bg} ${cfg.border}`;
  const interactive = onClick ? "cursor-pointer hover:brightness-125" : "";

  return (
    <span className="relative group inline-block">
      <span
        className={`font-medium rounded-full border ${sizeClass} ${activeBg} ${interactive} transition-colors`}
        onClick={onClick}
      >
        {cfg.text}
      </span>
      {!onClick && (
        <span className="pointer-events-none absolute bottom-full left-1/2 -translate-x-1/2 mb-2 px-3 py-1.5 rounded-lg text-xs text-[var(--text-primary)] bg-[var(--bg-panel-solid)] border border-[var(--border-glow)] shadow-[0_0_12px_rgba(59,130,246,0.15)] whitespace-nowrap opacity-0 group-hover:opacity-100 transition-opacity duration-150 z-50 max-w-[240px]">
          {cfg.description}
          <span className="absolute top-full left-1/2 -translate-x-1/2 border-4 border-transparent border-t-[var(--bg-panel-solid)]" />
        </span>
      )}
    </span>
  );
}
