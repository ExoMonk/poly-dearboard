import { useEffect, useRef } from "react";
import { useQueryClient } from "@tanstack/react-query";
import { useTerminalDispatch } from "../components/Terminal/TerminalProvider";
import type { CopyTradeUpdate } from "../types";

function truncateAssetId(id: string): string {
  if (id.length <= 12) return id;
  return `${id.slice(0, 6)}...${id.slice(-4)}`;
}

function updateKey(u: CopyTradeUpdate): string | null {
  switch (u.kind) {
    case "OrderPlaced":
      return `placed:${u.order.id}`;
    case "OrderFilled":
      return `filled:${u.order_id}`;
    case "OrderFailed":
      return `failed:${u.order_id}`;
    case "SessionPaused":
      return `paused:${u.session_id}`;
    case "SessionResumed":
      return `resumed:${u.session_id}`;
    case "SessionStopped":
      return `stopped:${u.session_id}`;
    case "BalanceUpdate":
      return null; // handled separately
  }
}

export function useCopyTradeLogger(updates: CopyTradeUpdate[]) {
  const { addLog } = useTerminalDispatch();
  const queryClient = useQueryClient();
  const processedRef = useRef<Set<string>>(new Set());
  const lastBalanceRef = useRef<string | null>(null);
  const seededRef = useRef(false);

  // Seed dedup set from initial updates on mount
  useEffect(() => {
    for (const u of updates) {
      const key = updateKey(u);
      if (key) processedRef.current.add(key);
      if (u.kind === "BalanceUpdate") lastBalanceRef.current = u.balance;
    }
    seededRef.current = true;
  // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  useEffect(() => {
    if (!seededRef.current) return;

    for (const u of updates) {
      const key = updateKey(u);

      if (u.kind === "BalanceUpdate") {
        if (u.balance === lastBalanceRef.current) continue;
        lastBalanceRef.current = u.balance;
        addLog("info", `Balance updated: $${u.balance} USDC`, { balance: u.balance }, "copytrade");
        continue;
      }

      if (key && processedRef.current.has(key)) continue;
      if (key) processedRef.current.add(key);

      switch (u.kind) {
        case "OrderPlaced":
          addLog(
            "info",
            `Copy order placed: ${u.order.side} $${u.order.size_usdc} on ${truncateAssetId(u.order.asset_id)}`,
            {
              order_id: u.order.id,
              session_id: u.session_id,
              asset_id: u.order.asset_id,
              side: u.order.side,
              size_usdc: String(u.order.size_usdc),
              price: String(u.order.price),
              source_trader: u.order.source_trader,
              simulate: String(u.order.simulate),
            },
            "copytrade",
          );
          queryClient.invalidateQueries({ queryKey: ["copytrade", "orders"] });
          break;
        case "OrderFilled":
          addLog(
            "success",
            `Order filled at $${u.fill_price} (slippage: ${u.slippage_bps}bps)`,
            {
              order_id: u.order_id,
              session_id: u.session_id,
              fill_price: String(u.fill_price),
              slippage_bps: String(u.slippage_bps),
            },
            "copytrade",
          );
          queryClient.invalidateQueries({ queryKey: ["copytrade", "orders"] });
          queryClient.invalidateQueries({ queryKey: ["copytrade", "stats"] });
          queryClient.invalidateQueries({ queryKey: ["copytrade", "sessions"] });
          break;
        case "OrderFailed":
          addLog(
            "error",
            `Order failed: ${u.error}`,
            { order_id: u.order_id, session_id: u.session_id, error: u.error },
            "copytrade",
          );
          queryClient.invalidateQueries({ queryKey: ["copytrade", "orders"] });
          queryClient.invalidateQueries({ queryKey: ["copytrade", "stats"] });
          break;
        case "SessionPaused":
          addLog("warn", "Session paused", { session_id: u.session_id }, "copytrade");
          break;
        case "SessionResumed":
          addLog("info", "Session resumed", { session_id: u.session_id }, "copytrade");
          break;
        case "SessionStopped":
          addLog(
            "warn",
            `Session stopped${u.reason ? ": " + u.reason : ""}`,
            {
              session_id: u.session_id,
              ...(u.reason ? { reason: u.reason } : {}),
            },
            "copytrade",
          );
          break;
      }
    }
  }, [updates, addLog]);
}
