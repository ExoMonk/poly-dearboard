import { useEffect, useRef } from "react";
import { motion } from "motion/react";
import { useTerminalDispatch, useTerminalState } from "./TerminalProvider";
import { TerminalHeader } from "./TerminalHeader";
import { TerminalSidebar } from "./TerminalSidebar";
import { TerminalLogs } from "./TerminalLogs";
import { TerminalStatusBar } from "./TerminalStatusBar";
import { TerminalCommandPrompt } from "./TerminalCommandPrompt";
import { WalletTab } from "./WalletTab";
import { SessionsTab } from "./SessionsTab";
import { OrdersTab } from "./OrdersTab";
import { useCopyTradeWs } from "../../hooks/useCopyTrade";
import useAlerts from "../../hooks/useAlerts";
import { useCopyTradeLogger } from "../../hooks/useCopyTradeLogger";
import { useAlertLogger } from "../../hooks/useAlertLogger";
import { useOrderToast } from "../../hooks/useOrderToast";

const HEIGHT_MAP = {
  collapsed: 40,
  half: "40vh",
  full: "80vh",
} as const;

export function TerminalShell() {
  const { height, activeTab, isOpen, unread } = useTerminalState();
  const { incrUnread, clearUnread, clearAllUnread } = useTerminalDispatch();

  const seenCopytradeKeysRef = useRef<Set<string>>(new Set());
  const seenAlertKeysRef = useRef<Set<string>>(new Set());

  // Keep streams active so collapsed mode can still track unread events.
  const { updates, connected: wsConnected } = useCopyTradeWs({ enabled: true });
  const { alerts, connected: alertsConnected } = useAlerts({ enabled: true });
  useCopyTradeLogger(updates);
  useOrderToast(updates);
  useAlertLogger(alerts);

  useEffect(() => {
    for (const update of updates) {
      const key = (() => {
        switch (update.kind) {
          case "OrderPlaced":
            return `placed:${update.order.id}`;
          case "OrderFilled":
            return `filled:${update.order_id}`;
          case "OrderFailed":
            return `failed:${update.order_id}`;
          case "SessionPaused":
            return `paused:${update.session_id}`;
          case "SessionResumed":
            return `resumed:${update.session_id}`;
          case "SessionStopped":
            return `stopped:${update.session_id}:${update.reason ?? "none"}`;
          case "BalanceUpdate":
            return `balance:${update.balance}`;
        }
      })();

      if (seenCopytradeKeysRef.current.has(key)) continue;
      seenCopytradeKeysRef.current.add(key);
      if (height === "collapsed") {
        incrUnread("copytrade", 1);
      }
    }
  }, [updates, height, incrUnread]);

  useEffect(() => {
    for (const alert of alerts) {
      const key = `${alert.tx_hash}:${alert.kind}`;
      if (seenAlertKeysRef.current.has(key)) continue;
      seenAlertKeysRef.current.add(key);
      if (height === "collapsed") {
        incrUnread("alert", 1);
      }
    }
  }, [alerts, height, incrUnread]);

  useEffect(() => {
    if (!isOpen) return;

    if (activeTab === "logs") {
      if (unread.copytrade > 0 || unread.alert > 0) {
        clearAllUnread();
      }
      return;
    }

    if ((activeTab === "sessions" || activeTab === "orders") && unread.copytrade > 0) {
      clearUnread("copytrade");
    }
  }, [activeTab, isOpen, unread.copytrade, unread.alert, clearAllUnread, clearUnread]);

  // Set CSS custom property for main content padding
  useEffect(() => {
    const px =
      height === "collapsed" ? "40px" : height === "half" ? "40vh" : "80vh";
    document.documentElement.style.setProperty("--terminal-height", px);
    return () => {
      document.documentElement.style.removeProperty("--terminal-height");
    };
  }, [height]);

  const tabContent = (() => {
    switch (activeTab) {
      case "logs":
        return <TerminalLogs />;
      case "wallet":
        return <WalletTab />;
      case "sessions":
        return <SessionsTab />;
      case "orders":
        return <OrdersTab />;
    }
  })();

  return (
    <motion.div
      className="fixed bottom-0 left-0 right-0 z-40 flex flex-col glass border-t border-[var(--border-glow)]"
      animate={{ height: HEIGHT_MAP[height] }}
      transition={{ type: "spring", stiffness: 300, damping: 30 }}
    >
      <TerminalHeader />

      {isOpen && (
        <>
          <div className="flex flex-1 min-h-0">
            <TerminalSidebar />
            <div className="flex-1 min-h-0 overflow-hidden">{tabContent}</div>
          </div>
          <TerminalCommandPrompt />
          <TerminalStatusBar wsConnected={wsConnected} alertsConnected={alertsConnected} />
        </>
      )}
    </motion.div>
  );
}
