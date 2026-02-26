export interface StreamUiControls {
  paused: boolean;
  minUsdc: number;
  compact: boolean;
}

export const DEFAULT_STREAM_CONTROLS: StreamUiControls = {
  paused: false,
  minUsdc: 0,
  compact: false,
};

export function loadStreamControls(storageKey: string): StreamUiControls {
  try {
    const raw = localStorage.getItem(storageKey);
    if (!raw) return DEFAULT_STREAM_CONTROLS;
    const parsed = JSON.parse(raw) as Partial<StreamUiControls>;
    return {
      paused: Boolean(parsed.paused),
      minUsdc: Number.isFinite(parsed.minUsdc) ? Math.max(0, Number(parsed.minUsdc)) : 0,
      compact: Boolean(parsed.compact),
    };
  } catch {
    return DEFAULT_STREAM_CONTROLS;
  }
}

export function saveStreamControls(storageKey: string, controls: StreamUiControls): void {
  localStorage.setItem(storageKey, JSON.stringify(controls));
}

export function freshnessLabel(lastEventAt: number | null, nowTs: number): "Updated <10s" | "Stale" {
  if (!lastEventAt) return "Stale";
  return nowTs - lastEventAt < 10_000 ? "Updated <10s" : "Stale";
}
