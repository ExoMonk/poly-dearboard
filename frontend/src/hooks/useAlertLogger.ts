import { useEffect, useRef } from "react";
import { useTerminalDispatch } from "../components/Terminal/TerminalProvider";
import type { Alert } from "../types";

function truncateAddress(addr: string): string {
  if (addr.length <= 12) return addr;
  return `${addr.slice(0, 6)}...${addr.slice(-4)}`;
}

function truncateQuestion(q: string, max = 50): string {
  return q.length > max ? q.slice(0, max) + "..." : q;
}

function alertKey(a: Alert): string {
  return `${a.tx_hash}:${a.kind}`;
}

export function useAlertLogger(alerts: Alert[]) {
  const { addLog } = useTerminalDispatch();
  const processedRef = useRef<Set<string>>(new Set());
  const seededRef = useRef(false);

  // Seed dedup set from initial alerts on mount
  useEffect(() => {
    for (const a of alerts) {
      processedRef.current.add(alertKey(a));
    }
    seededRef.current = true;
  // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  useEffect(() => {
    if (!seededRef.current) return;

    for (const a of alerts) {
      const key = alertKey(a);
      if (processedRef.current.has(key)) continue;
      processedRef.current.add(key);

      switch (a.kind) {
        case "WhaleTrade":
          addLog(
            "warn",
            `Whale ${a.side} $${a.usdc_amount} by ${truncateAddress(a.trader)}`,
            {
              tx_hash: a.tx_hash,
              trader: a.trader,
              asset_id: a.asset_id,
              usdc_amount: a.usdc_amount,
              token_amount: a.token_amount,
              side: a.side,
              block_number: String(a.block_number),
              ...(a.question ? { question: a.question } : {}),
              ...(a.outcome ? { outcome: a.outcome } : {}),
            },
            "alert",
          );
          break;
        case "MarketResolution":
          addLog(
            "info",
            `Market resolved: ${a.question ? truncateQuestion(a.question) : a.condition_id.slice(0, 10) + "..."}${a.winning_outcome ? " \u2192 " + a.winning_outcome : ""}`,
            {
              tx_hash: a.tx_hash,
              condition_id: a.condition_id,
              block_number: String(a.block_number),
              ...(a.question ? { question: a.question } : {}),
              ...(a.winning_outcome ? { winning_outcome: a.winning_outcome } : {}),
              ...(a.token_id ? { token_id: a.token_id } : {}),
            },
            "alert",
          );
          break;
        case "FailedSettlement":
          addLog(
            "error",
            `Settlement failed: ${a.function_name} (gas: ${a.gas_used})`,
            {
              tx_hash: a.tx_hash,
              block_number: String(a.block_number),
              from_address: a.from_address,
              to_contract: a.to_contract,
              function_name: a.function_name,
              gas_used: a.gas_used,
            },
            "alert",
          );
          break;
      }
    }
  }, [alerts, addLog]);
}
