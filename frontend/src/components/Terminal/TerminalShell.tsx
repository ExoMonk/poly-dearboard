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
import { AlertsTab } from "./AlertsTab";
import { LiveFeedTab } from "./LiveFeedTab";
import { DetachedPanel } from "./DetachedPanel";
import { useCopyTradeWs } from "../../hooks/useCopyTrade";
import useAlerts from "../../hooks/useAlerts";
import useSignalFeed from "../../hooks/useSignalFeed";
import useTradeWs from "../../hooks/useTradeWs";
import { useCopyTradeLogger } from "../../hooks/useCopyTradeLogger";
import { useAlertLogger } from "../../hooks/useAlertLogger";
import { useOrderToast } from "../../hooks/useOrderToast";

const HEIGHT_MAP = {
  collapsed: 40,
  half: "50vh",
  full: "80vh",
} as const;

export function TerminalShell() {
  const {
    height, activeTab, isOpen, unread,
    detachedTabs, minimizedPanels, liveFeedMode, liveFeedListId,
  } = useTerminalState();
  const {
    incrUnread, clearUnread, clearAllUnread,
    attachTab, minimizePanel, restorePanel, setLiveFeedMode, setLiveFeedListId,
  } = useTerminalDispatch();

  const seenCopytradeKeysRef = useRef<Set<string>>(new Set());
  const seenAlertKeysRef = useRef<Set<string>>(new Set());

  // Always-on streams (existing)
  const { updates, connected: wsConnected } = useCopyTradeWs({ enabled: true });
  const { alerts, connected: alertsConnected } = useAlerts({ enabled: true });
  useCopyTradeLogger(updates);
  useOrderToast(updates);
  useAlertLogger(alerts);

  // Lazy feed streams — only connect when feed tab is active or detached
  const feedActive = activeTab === "feed" || detachedTabs.includes("feed");
  const signalFeed = useSignalFeed({
    enabled: feedActive && liveFeedMode === "signals",
    listId: liveFeedListId,
    topN: liveFeedListId ? undefined : 20,
  });
  const publicFeed = useTradeWs({
    enabled: feedActive && liveFeedMode === "public",
  });

  // --- Unread tracking ---
  const alertsVisible =
    (isOpen && activeTab === "alerts") ||
    (detachedTabs.includes("alerts") && !minimizedPanels.includes("alerts"));

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
      if (!alertsVisible) {
        incrUnread("alert", 1);
      }
    }
  }, [alerts, alertsVisible, incrUnread]);

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

    if (activeTab === "alerts" && unread.alert > 0) {
      clearUnread("alert");
    }
  }, [activeTab, isOpen, unread.copytrade, unread.alert, clearAllUnread, clearUnread]);

  // Set CSS custom property for main content padding
  useEffect(() => {
    const px =
      height === "collapsed" ? "40px" : height === "half" ? "50vh" : "80vh";
    document.documentElement.style.setProperty("--terminal-height", px);
    return () => {
      document.documentElement.style.removeProperty("--terminal-height");
    };
  }, [height]);

  const feedConnected = liveFeedMode === "signals" ? signalFeed.connected : publicFeed.connected;

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
      case "feed":
        if (detachedTabs.includes("feed")) {
          return <DetachedPlaceholder label="LiveFeed" onReattach={() => attachTab("feed")} />;
        }
        return (
          <LiveFeedTab
            mode={liveFeedMode}
            onSetMode={setLiveFeedMode}
            listId={liveFeedListId}
            onSetListId={setLiveFeedListId}
            connected={feedConnected}
            isLagging={signalFeed.isLagging}
            signalTrades={signalFeed.trades}
            convergenceAlerts={signalFeed.alerts}
            publicTrades={publicFeed.liveTrades}
          />
        );
      case "alerts":
        if (detachedTabs.includes("alerts")) {
          return <DetachedPlaceholder label="Alerts" onReattach={() => attachTab("alerts")} />;
        }
        return <AlertsTab alerts={alerts} connected={alertsConnected} />;
    }
  })();

  return (
    <>
      <motion.div
        className={`fixed bottom-0 left-0 right-0 z-40 flex flex-col ${
          height === "collapsed"
            ? "border-t border-transparent bg-transparent"
            : "border-t border-[var(--border-glow)] glass"
        }`}
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

      {/* Detached floating panels */}
      {detachedTabs.includes("feed") && (
        <DetachedPanel
          tabId="feed"
          title="LiveFeed"
          connected={feedConnected}
          minimized={minimizedPanels.includes("feed")}
          onClose={() => attachTab("feed")}
          onMinimize={() => minimizePanel("feed")}
          onRestore={() => restorePanel("feed")}
        >
          <LiveFeedTab
            mode={liveFeedMode}
            onSetMode={setLiveFeedMode}
            listId={liveFeedListId}
            onSetListId={setLiveFeedListId}
            connected={feedConnected}
            isLagging={signalFeed.isLagging}
            signalTrades={signalFeed.trades}
            convergenceAlerts={signalFeed.alerts}
            publicTrades={publicFeed.liveTrades}
          />
        </DetachedPanel>
      )}
      {detachedTabs.includes("alerts") && (
        <DetachedPanel
          tabId="alerts"
          title="Alerts"
          connected={alertsConnected}
          minimized={minimizedPanels.includes("alerts")}
          onClose={() => attachTab("alerts")}
          onMinimize={() => minimizePanel("alerts")}
          onRestore={() => restorePanel("alerts")}
        >
          <AlertsTab alerts={alerts} connected={alertsConnected} />
        </DetachedPanel>
      )}
    </>
  );
}

function DetachedPlaceholder({ label, onReattach }: { label: string; onReattach: () => void }) {
  return (
    <div className="flex flex-col items-center justify-center h-full gap-3 text-[var(--text-muted)]">
      <svg className="w-6 h-6" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
        <path d="M15 3h6v6M9 21H3v-6M21 3l-7 7M3 21l7-7" />
      </svg>
      <span className="text-xs">{label} is detached</span>
      <button
        onClick={onReattach}
        className="text-xs px-3 py-1 rounded border border-white/10 hover:bg-white/5 transition-colors"
      >
        Re-attach
      </button>
    </div>
  );
}
