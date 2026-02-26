import { useState, useEffect, useRef, useCallback } from "react";
import type { FeedTrade } from "../types";

const MAX_TRADES = 200;
const RECONNECT_BASE_MS = 1000;
const RECONNECT_MAX_MS = 30000;

interface Params {
  tokenIds?: string;
  enabled?: boolean;
}

export default function useTradeWs({ tokenIds = "", enabled = true }: Params) {
  const [liveTrades, setLiveTrades] = useState<FeedTrade[]>([]);
  const [connected, setConnected] = useState(false);
  const [lastEventAt, setLastEventAt] = useState<number | null>(null);
  const wsRef = useRef<WebSocket | null>(null);
  const retryRef = useRef(0);
  const reconnectTimerRef = useRef<ReturnType<typeof setTimeout> | undefined>(undefined);

  const connect = useCallback(() => {
    if (!enabled) return;

    // WS routes live outside /api nest — use origin only, strip path
    const base = import.meta.env.VITE_API_URL || "";
    const wsBase = base
      ? new URL(base).origin.replace(/^http/, "ws")
      : `${window.location.protocol === "https:" ? "wss:" : "ws:"}//${window.location.host}`;
    // Empty tokenIds = subscribe to all trades (backend wildcard)
    const params = tokenIds ? `?token_ids=${encodeURIComponent(tokenIds)}` : "";
    const url = `${wsBase}/ws/trades${params}`;

    const ws = new WebSocket(url);
    wsRef.current = ws;

    ws.onopen = () => {
      setConnected(true);
      retryRef.current = 0;
    };

    ws.onmessage = (event) => {
      try {
        const trade: FeedTrade = JSON.parse(event.data);
        setLastEventAt(Date.now());
        setLiveTrades((prev) => {
          if (prev.some((t) => t.tx_hash === trade.tx_hash)) return prev;
          return [trade, ...prev].slice(0, MAX_TRADES);
        });
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
  }, [tokenIds, enabled]);

  useEffect(() => {
    if (!enabled) {
      wsRef.current?.close();
      wsRef.current = null;
      clearTimeout(reconnectTimerRef.current);
      setConnected(false);
      setLiveTrades([]);
      setLastEventAt(null);
      retryRef.current = 0;
      return;
    }
    setLastEventAt(null);
    connect();
    return () => {
      clearTimeout(reconnectTimerRef.current);
      wsRef.current?.close();
    };
  }, [connect, enabled]);

  return { liveTrades, connected, lastEventAt };
}
