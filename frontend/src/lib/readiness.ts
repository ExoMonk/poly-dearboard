import type { DiscoveryCategory, ReadinessBucket, TraderReadiness } from "../types";

// --- Reason code → display copy ---

export const REASON_COPY: Record<string, { short: string; detail: string }> = {
  high_confidence: {
    short: "High confidence",
    detail: "Statistically significant sample with 50+ settled positions",
  },
  strong_win_rate: {
    short: "Strong win rate",
    detail: "Win rate above 60% with meaningful sample size",
  },
  strong_consistency: {
    short: "Consistent edge",
    detail: "Statistically significant edge above random chance",
  },
  active_recently: {
    short: "Active recently",
    detail: "Traded within the last 7 days",
  },
  volume_maker_penalty: {
    short: "Volume maker",
    detail:
      "High volume in near-resolved markets — may not reflect directional edge",
  },
  low_sample_penalty: {
    short: "Low sample",
    detail: "Fewer than 10 settled positions — results may not be representative",
  },
  low_activity_penalty: {
    short: "Limited history",
    detail: "Limited market breadth or trading history",
  },
};

// --- Category → display copy ---

export const CATEGORY_CONFIG: Record<
  DiscoveryCategory,
  { text: string; bg: string; border: string; description: string }
> = {
  momentum: {
    text: "Momentum",
    bg: "bg-amber-500/10",
    border: "border-amber-500/30",
    description: "Positive PnL, active recently, above-average win rate",
  },
  consistent: {
    text: "Consistent",
    bg: "bg-emerald-500/10",
    border: "border-emerald-500/30",
    description: "Steady win rate across many settled markets",
  },
  high_conviction: {
    text: "High Conviction",
    bg: "bg-violet-500/10",
    border: "border-violet-500/30",
    description: "Large average position sizes, concentrated portfolio",
  },
  fast_mover: {
    text: "Fast Mover",
    bg: "bg-sky-500/10",
    border: "border-sky-500/30",
    description: "High trade frequency over a short active period",
  },
  contrarian: {
    text: "Contrarian",
    bg: "bg-rose-500/10",
    border: "border-rose-500/30",
    description: "Buys cheap outcomes that settle correctly",
  },
  volume_maker: {
    text: "Volume Maker",
    bg: "bg-zinc-500/10",
    border: "border-zinc-500/30",
    description:
      "High volume concentrated in near-resolved markets — edge may be limited",
  },
};

// --- Bucket display ---

export const BUCKET_CONFIG: Record<
  ReadinessBucket,
  { text: string; color: string; bg: string; border: string }
> = {
  high: {
    text: "High",
    color: "text-emerald-400",
    bg: "bg-emerald-500/10",
    border: "border-emerald-500/30",
  },
  medium: {
    text: "Medium",
    color: "text-amber-400",
    bg: "bg-amber-500/10",
    border: "border-amber-500/30",
  },
  low: {
    text: "Low",
    color: "text-zinc-400",
    bg: "bg-zinc-500/10",
    border: "border-zinc-500/30",
  },
};

// --- Helpers ---

export function reasonDisplay(code: string, fallback?: string) {
  const entry = REASON_COPY[code];
  return entry ?? { short: fallback ?? code, detail: fallback ?? code };
}

export function readinessTooltip(r: TraderReadiness): string {
  const parts = r.reason_codes.slice(0, 3).map((c, i) => {
    const entry = REASON_COPY[c];
    return entry?.short ?? r.reasons[i] ?? c;
  });
  return `Score ${r.score}/100 — ${parts.join(", ")}`;
}
