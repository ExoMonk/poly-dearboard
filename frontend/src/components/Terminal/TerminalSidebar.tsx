import { useTerminalState, useTerminalDispatch } from "./TerminalProvider";
import type { TerminalTab } from "../../types";

interface TabDef {
  id: TerminalTab;
  label: string;
  shortcutKey?: string;
  icon: React.ReactNode;
  detachable?: boolean;
}

const isMac = /Mac|iPhone|iPad/.test(navigator.userAgent);
const altPrefix = isMac ? "\u2325" : "Alt+";

const TABS: TabDef[] = [
  {
    id: "wallet",
    label: "Wallet",
    shortcutKey: "1",
    icon: (
      <svg className="w-4 h-4" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
        <rect x="2" y="6" width="20" height="14" rx="2" />
        <path d="M2 10h20" />
        <path d="M6 2v4" />
        <path d="M18 2v4" />
      </svg>
    ),
  },
  {
    id: "sessions",
    label: "Sessions",
    shortcutKey: "2",
    icon: (
      <svg className="w-4 h-4" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
        <path d="M3 3v18h18" />
        <path d="M7 16l4-8 4 4 4-6" />
      </svg>
    ),
  },
  {
    id: "logs",
    label: "Logs",
    shortcutKey: "3",
    icon: (
      <svg className="w-4 h-4" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
        <path d="M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z" />
        <polyline points="14 2 14 8 20 8" />
        <line x1="16" y1="13" x2="8" y2="13" />
        <line x1="16" y1="17" x2="8" y2="17" />
      </svg>
    ),
  },
  {
    id: "orders",
    label: "Orders",
    shortcutKey: "4",
    icon: (
      <svg className="w-4 h-4" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
        <path d="M21 16V8a2 2 0 0 0-1-1.73l-7-4a2 2 0 0 0-2 0l-7 4A2 2 0 0 0 3 8v8a2 2 0 0 0 1 1.73l7 4a2 2 0 0 0 2 0l7-4A2 2 0 0 0 21 16z" />
        <polyline points="3.27 6.96 12 12.01 20.73 6.96" />
        <line x1="12" y1="22.08" x2="12" y2="12" />
      </svg>
    ),
  },
  {
    id: "feed",
    label: "LiveFeed",
    shortcutKey: "5",
    detachable: true,
    icon: (
      <svg className="w-4 h-4" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
        <path d="M4 11a9 9 0 0 1 9 9" />
        <path d="M4 4a16 16 0 0 1 16 16" />
        <circle cx="5" cy="19" r="1" />
      </svg>
    ),
  },
  {
    id: "alerts",
    label: "Alerts",
    shortcutKey: "6",
    detachable: true,
    icon: (
      <svg className="w-4 h-4" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
        <path d="M18 8A6 6 0 0 0 6 8c0 7-3 9-3 9h18s-3-2-3-9" />
        <path d="M13.73 21a2 2 0 0 1-3.46 0" />
      </svg>
    ),
  },
];

export function TerminalSidebar() {
  const { activeTab, detachedTabs } = useTerminalState();
  const { setActiveTab, detachTab, attachTab } = useTerminalDispatch();

  return (
    <div className="w-[140px] shrink-0 border-r border-white/5 py-1 overflow-y-auto">
      {TABS.map((tab) => {
        const isActive = activeTab === tab.id;
        const isDetached = detachedTabs.includes(tab.id);
        return (
          <div key={tab.id} className="relative group">
            <button
              className={`w-full flex items-center gap-2 px-3 py-2 text-xs transition-colors ${
                isDetached
                  ? "text-[var(--text-muted)]/50 border-l-2 border-transparent"
                  : isActive
                    ? "text-[var(--accent-blue)] bg-[var(--accent-blue)]/10 border-l-2 border-[var(--accent-blue)]"
                    : "text-[var(--text-muted)] hover:text-[var(--text-secondary)] hover:bg-white/5 border-l-2 border-transparent"
              }`}
              onClick={() => {
                if (isDetached) {
                  attachTab(tab.id);
                  setActiveTab(tab.id);
                } else {
                  setActiveTab(tab.id);
                }
              }}
            >
              {tab.icon}
              <span className={isDetached ? "line-through" : ""}>{tab.label}</span>
              {tab.shortcutKey && !isDetached && (
                <span className="ml-auto text-[10px] text-[var(--text-secondary)]/80 font-mono">
                  {`${altPrefix}${tab.shortcutKey}`}
                </span>
              )}
              {isDetached && (
                <span className="ml-auto text-[9px] text-[var(--text-muted)]">pop</span>
              )}
            </button>
            {/* Detach button — hidden on mobile, only for detachable tabs */}
            {tab.detachable && !isDetached && (
              <button
                onClick={(e) => { e.stopPropagation(); detachTab(tab.id); }}
                className="absolute right-1 top-1/2 -translate-y-1/2 p-0.5 rounded opacity-0 group-hover:opacity-100 hover:bg-white/10 text-[var(--text-muted)] transition-opacity hidden md:block"
                title={`Detach ${tab.label}`}
              >
                <svg className="w-3 h-3" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
                  <path d="M15 3h6v6M9 21H3v-6M21 3l-7 7M3 21l7-7" />
                </svg>
              </button>
            )}
          </div>
        );
      })}
    </div>
  );
}
