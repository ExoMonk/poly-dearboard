import { useCallback, useSyncExternalStore } from "react";

const STORAGE_KEY = "pd_recent_traders";
const MAX_RECENT = 10;

interface RecentEntry {
  address: string;
  visitedAt: number;
}

let listeners: (() => void)[] = [];
let cachedRaw: string | null = null;
let cachedEntries: RecentEntry[] = [];

function emit() {
  listeners.forEach((l) => l());
}

function getSnapshot(): RecentEntry[] {
  try {
    const raw = localStorage.getItem(STORAGE_KEY);
    if (raw !== cachedRaw) {
      cachedRaw = raw;
      cachedEntries = raw ? JSON.parse(raw) : [];
    }
    return cachedEntries;
  } catch {
    return cachedEntries;
  }
}

function subscribe(listener: () => void) {
  listeners.push(listener);
  return () => {
    listeners = listeners.filter((l) => l !== listener);
  };
}

export function useRecentTraders() {
  const entries = useSyncExternalStore(subscribe, getSnapshot, getSnapshot);

  const recordVisit = useCallback((address: string) => {
    const addr = address.toLowerCase();
    const current = getSnapshot().filter((e) => e.address !== addr);
    const next = [{ address: addr, visitedAt: Date.now() }, ...current].slice(0, MAX_RECENT);
    localStorage.setItem(STORAGE_KEY, JSON.stringify(next));
    emit();
  }, []);

  return { recentTraders: entries, recordVisit };
}
