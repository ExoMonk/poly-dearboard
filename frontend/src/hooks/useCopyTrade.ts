import { useState, useEffect, useRef, useCallback } from "react";
import { useQuery, useMutation, useQueryClient } from "@tanstack/react-query";
import * as api from "../api";
import type { CopyTradeSession, CopyTradeOrder, CopyTradeUpdate, CreateSessionRequest, SessionStats, CopyTradePosition, CopyTradeSummary } from "../types";

const JWT_KEY = "pd_jwt";
const RECONNECT_BASE_MS = 1000;
const RECONNECT_MAX_MS = 30000;
const MAX_UPDATES = 100;

interface CopyTradeWsOptions {
  enabled?: boolean;
}

// -- Query hooks --

export function useSessions() {
  const hasJwt = !!localStorage.getItem(JWT_KEY);
  return useQuery<CopyTradeSession[]>({
    queryKey: ["copytrade", "sessions"],
    queryFn: api.listSessions,
    enabled: hasJwt,
    refetchInterval: 10_000,
  });
}

export function useSession(id: string | null) {
  return useQuery<CopyTradeSession>({
    queryKey: ["copytrade", "sessions", id],
    queryFn: () => api.getSession(id!),
    enabled: !!id,
    refetchInterval: 5_000,
  });
}

export function useSessionOrders(sessionId: string | null, limit = 50) {
  return useQuery<CopyTradeOrder[]>({
    queryKey: ["copytrade", "orders", sessionId, limit],
    queryFn: () => api.listSessionOrders(sessionId!, limit),
    enabled: !!sessionId,
    refetchInterval: 5_000,
  });
}

export function useSessionStats(sessionId: string | null) {
  return useQuery<SessionStats>({
    queryKey: ["copytrade", "stats", sessionId],
    queryFn: () => api.getSessionStats(sessionId!),
    enabled: !!sessionId,
    refetchInterval: 15_000,
  });
}

export function useSessionPositions(sessionId: string | null) {
  return useQuery<CopyTradePosition[]>({
    queryKey: ["copytrade", "positions", sessionId],
    queryFn: () => api.getSessionPositions(sessionId!),
    enabled: !!sessionId,
    refetchInterval: 10_000,
  });
}

export function useCopyTradeSummary() {
  const hasJwt = !!localStorage.getItem(JWT_KEY);
  return useQuery<CopyTradeSummary>({
    queryKey: ["copytrade", "summary"],
    queryFn: api.getCopyTradeSummary,
    enabled: hasJwt,
    refetchInterval: 15_000,
  });
}

export function useActiveTraders() {
  const hasJwt = !!localStorage.getItem(JWT_KEY);
  return useQuery<string[]>({
    queryKey: ["copytrade", "active-traders"],
    queryFn: api.getActiveTraders,
    enabled: hasJwt,
    refetchInterval: 30_000,
  });
}

// -- Mutation hooks --

export function useCreateSession() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (body: CreateSessionRequest) => api.createSession(body),
    onSuccess: () => qc.invalidateQueries({ queryKey: ["copytrade", "sessions"] }),
  });
}

export function useUpdateSession() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: ({ id, action }: { id: string; action: "pause" | "resume" | "stop" }) =>
      api.updateSession(id, action),
    onSuccess: () => qc.invalidateQueries({ queryKey: ["copytrade"] }),
  });
}

export function useDeleteSession() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (id: string) => api.deleteSession(id),
    onSuccess: () => qc.invalidateQueries({ queryKey: ["copytrade", "sessions"] }),
  });
}

export function useClosePosition() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: ({ sessionId, assetId }: { sessionId: string; assetId: string }) =>
      api.closePosition(sessionId, assetId),
    onSuccess: () => qc.invalidateQueries({ queryKey: ["copytrade"] }),
  });
}

export function useRedeemPosition() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: ({ sessionId, assetId }: { sessionId: string; assetId: string }) =>
      api.redeemPosition(sessionId, assetId),
    onSuccess: () => qc.invalidateQueries({ queryKey: ["copytrade"] }),
  });
}

// -- WebSocket hook --

export function useCopyTradeWs(options: CopyTradeWsOptions = {}) {
  const { enabled = true } = options;
  const [updates, setUpdates] = useState<CopyTradeUpdate[]>([]);
  const [connected, setConnected] = useState(false);
  const wsRef = useRef<WebSocket | null>(null);
  const retryRef = useRef(0);
  const reconnectTimerRef = useRef<number | null>(null);
  const connectRef = useRef<() => void>(() => {});

  const connect = useCallback(() => {
    if (!enabled) return;

    const token = localStorage.getItem(JWT_KEY);
    if (!token) return;

    const base = import.meta.env.VITE_API_URL || "";
    const wsBase = base
      ? new URL(base).origin.replace(/^http/, "ws")
      : `${window.location.protocol === "https:" ? "wss:" : "ws:"}//${window.location.host}`;
    const url = `${wsBase}/ws/copytrade?token=${encodeURIComponent(token)}`;

    const ws = new WebSocket(url);
    wsRef.current = ws;

    ws.onopen = () => {
      setConnected(true);
      retryRef.current = 0;
    };

    ws.onmessage = (event) => {
      try {
        const update: CopyTradeUpdate = JSON.parse(event.data);
        setUpdates((prev) => [update, ...prev].slice(0, MAX_UPDATES));
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
      reconnectTimerRef.current = window.setTimeout(() => {
        connectRef.current();
      }, delay);
    };

    ws.onerror = () => {
      ws.close();
    };
  }, [enabled]);

  useEffect(() => {
    connectRef.current = connect;
  }, [connect]);

  useEffect(() => {
    if (!enabled) {
      if (reconnectTimerRef.current !== null) {
        clearTimeout(reconnectTimerRef.current);
        reconnectTimerRef.current = null;
      }
      wsRef.current?.close();
      wsRef.current = null;
      setConnected(false);
      setUpdates([]);
      retryRef.current = 0;
      return;
    }

    connect();
    return () => {
      if (reconnectTimerRef.current !== null) {
        clearTimeout(reconnectTimerRef.current);
        reconnectTimerRef.current = null;
      }
      wsRef.current?.close();
      wsRef.current = null;
      setConnected(false);
    };
  }, [connect, enabled]);

  const clearUpdates = useCallback(() => setUpdates([]), []);

  return { updates, connected, clearUpdates };
}
