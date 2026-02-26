import { useState, useEffect, useRef, useCallback } from "react";
import type { SignalTrade, ConvergenceAlert, SignalMessage } from "../types";

const MAX_TRADES = 200;
const MAX_ALERTS = 50;
const RECONNECT_BASE_MS = 1000;
const RECONNECT_MAX_MS = 30000;
const JWT_KEY = "pd_jwt";

interface Params {
  listId: string | null;
  topN?: number;
  enabled?: boolean;
}

export default function useSignalFeed({ listId, topN, enabled = true }: Params) {
  const [trades, setTrades] = useState<SignalTrade[]>([]);
  const [alerts, setAlerts] = useState<ConvergenceAlert[]>([]);
  const [connected, setConnected] = useState(false);
  const [isLagging, setIsLagging] = useState(false);
  const [lastEventAt, setLastEventAt] = useState<number | null>(null);
  const wsRef = useRef<WebSocket | null>(null);
  const retryRef = useRef(0);
  const reconnectTimerRef = useRef<ReturnType<typeof setTimeout> | undefined>(undefined);

  const connect = useCallback(() => {
    if (!enabled) return;
    const token = localStorage.getItem(JWT_KEY);
    if (!token) return;
    // Need either a list or top_n
    if (!listId && !topN) return;

    const base = import.meta.env.VITE_API_URL || "";
    const wsBase = base
      ? new URL(base).origin.replace(/^http/, "ws")
      : `${window.location.protocol === "https:" ? "wss:" : "ws:"}//${window.location.host}`;
    const sourceParam = listId
      ? `list_id=${encodeURIComponent(listId)}`
      : `top_n=${topN}`;
    const url = `${wsBase}/ws/signals?${sourceParam}&token=${encodeURIComponent(token)}`;

    const ws = new WebSocket(url);
    wsRef.current = ws;

    ws.onopen = () => {
      setConnected(true);
      setIsLagging(false);
      retryRef.current = 0;
    };

    ws.onmessage = (event) => {
      try {
        const msg: SignalMessage = JSON.parse(event.data);
        setLastEventAt(Date.now());
        switch (msg.kind) {
          case "Trade":
            setTrades((prev) => {
              if (prev.some((t) => t.tx_hash === msg.tx_hash)) return prev;
              return [msg, ...prev].slice(0, MAX_TRADES);
            });
            break;
          case "Convergence":
            setAlerts((prev) => [msg, ...prev].slice(0, MAX_ALERTS));
            break;
          case "Lag":
            setIsLagging(true);
            break;
        }
      } catch {
        // Ignore malformed messages
      }
    };

    ws.onclose = () => {
      setConnected(false);
      wsRef.current = null;
      if (!enabled) return;
      const delay = Math.min(
        RECONNECT_BASE_MS * Math.pow(2, retryRef.current),
        RECONNECT_MAX_MS,
      );
      retryRef.current++;
      reconnectTimerRef.current = setTimeout(connect, delay);
    };

    ws.onerror = () => {
      ws.close();
    };
  }, [listId, topN, enabled]);

  useEffect(() => {
    if (!enabled) {
      wsRef.current?.close();
      wsRef.current = null;
      clearTimeout(reconnectTimerRef.current);
      setConnected(false);
      setTrades([]);
      setAlerts([]);
      setIsLagging(false);
      setLastEventAt(null);
      retryRef.current = 0;
      return;
    }
    setTrades([]);
    setAlerts([]);
    setIsLagging(false);
    setLastEventAt(null);
    connect();
    return () => {
      clearTimeout(reconnectTimerRef.current);
      wsRef.current?.close();
    };
  }, [connect, enabled]);

  return { trades, alerts, connected, isLagging, lastEventAt };
}
