import { useEffect, useRef } from "react";
import { useToast } from "../components/Toast";
import type { CopyTradeUpdate } from "../types";

function truncate(id: string): string {
  if (id.length <= 12) return id;
  return `${id.slice(0, 6)}…${id.slice(-4)}`;
}

export function useOrderToast(updates: CopyTradeUpdate[]) {
  const { toast } = useToast();
  const seenRef = useRef<Set<string>>(new Set());
  const seededRef = useRef(false);

  // Seed dedup set on mount (don't toast old events)
  useEffect(() => {
    for (const u of updates) {
      const key = toastKey(u);
      if (key) seenRef.current.add(key);
    }
    seededRef.current = true;
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  useEffect(() => {
    if (!seededRef.current) return;

    for (const u of updates) {
      const key = toastKey(u);
      if (!key || seenRef.current.has(key)) continue;
      seenRef.current.add(key);

      switch (u.kind) {
        case "OrderPlaced":
          toast(
            "info",
            `${u.order.side} $${u.order.size_usdc.toFixed(2)} on ${truncate(u.order.asset_id)}`,
          );
          break;
        case "OrderFilled":
          toast("success", `Filled @ $${u.fill_price.toFixed(4)}`);
          break;
        case "OrderFailed":
          toast("error", `Order failed: ${u.error}`);
          break;
        case "SessionStopped": {
          const reason = (u.reason ?? "").trim();
          const normalized = reason.toLowerCase();

          if (
            normalized.includes("circuit_breaker")
            || normalized.includes("max_loss")
            || normalized.includes("max loss")
            || normalized.includes("loss exceeds")
            || normalized.includes("auto-stopped")
          ) {
            toast("error", "Session auto-stopped (circuit breaker).");
            break;
          }

          if (
            normalized.includes("clob init failed")
            || normalized.includes("trader resolution failed")
          ) {
            toast("error", `Session stopped: ${reason || "infrastructure error"}`);
            break;
          }

          if (normalized === "user" || !normalized) {
            toast("warn", "Session stopped.");
            break;
          }

          toast("warn", `Session stopped: ${reason}`);
          break;
        }
      }
    }
  }, [updates, toast]);
}

function toastKey(u: CopyTradeUpdate): string | null {
  switch (u.kind) {
    case "OrderPlaced":
      return `t:placed:${u.order.id}`;
    case "OrderFilled":
      return `t:filled:${u.order_id}`;
    case "OrderFailed":
      return `t:failed:${u.order_id}`;
    case "SessionStopped":
      return `t:stopped:${u.session_id}:${u.reason ?? "none"}`;
    default:
      return null;
  }
}
