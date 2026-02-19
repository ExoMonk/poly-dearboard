import { useState, useEffect, useRef, useCallback } from "react";
import type { FeedTrade } from "../types";

const MAX_TRADES = 200;
const RECONNECT_BASE_MS = 1000;
const RECONNECT_MAX_MS = 30000;

interface Params {
  tokenIds: string;
}

export default function useTradeWs({ tokenIds }: Params) {
  const [liveTrades, setLiveTrades] = useState<FeedTrade[]>([]);
  const [connected, setConnected] = useState(false);
  const wsRef = useRef<WebSocket | null>(null);
  const retryRef = useRef(0);

  const connect = useCallback(() => {
    if (!tokenIds) return;

    // WS routes live outside /api nest — use origin only, strip path
    const base = import.meta.env.VITE_API_URL || "";
    const wsBase = base
      ? new URL(base).origin.replace(/^http/, "ws")
      : `${window.location.protocol === "https:" ? "wss:" : "ws:"}//${window.location.host}`;
    const url = `${wsBase}/ws/trades?token_ids=${encodeURIComponent(tokenIds)}`;

    const ws = new WebSocket(url);
    wsRef.current = ws;

    ws.onopen = () => {
      setConnected(true);
      retryRef.current = 0;
    };

    ws.onmessage = (event) => {
      try {
        const trade: FeedTrade = JSON.parse(event.data);
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
      const delay = Math.min(
        RECONNECT_BASE_MS * Math.pow(2, retryRef.current),
        RECONNECT_MAX_MS,
      );
      retryRef.current++;
      setTimeout(connect, delay);
    };

    ws.onerror = () => {
      ws.close();
    };
  }, [tokenIds]);

  useEffect(() => {
    connect();
    return () => {
      wsRef.current?.close();
    };
  }, [connect]);

  return { liveTrades, connected };
}
