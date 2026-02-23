import { useEffect } from "react";
import { motion } from "motion/react";
import { useTerminalState } from "./TerminalProvider";
import { TerminalHeader } from "./TerminalHeader";
import { TerminalSidebar } from "./TerminalSidebar";
import { TerminalLogs } from "./TerminalLogs";
import { TerminalStatusBar } from "./TerminalStatusBar";
import { WalletTab } from "./WalletTab";
import { SessionsTab } from "./SessionsTab";
import { OrdersTab } from "./OrdersTab";

const HEIGHT_MAP = {
  collapsed: 40,
  half: "40vh",
  full: "80vh",
} as const;

export function TerminalShell() {
  const { height, activeTab, isOpen } = useTerminalState();

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
          <TerminalStatusBar />
        </>
      )}
    </motion.div>
  );
}
