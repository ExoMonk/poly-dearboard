import {
  createContext,
  useContext,
  useEffect,
  useMemo,
  useReducer,
  type ReactNode,
} from "react";
import type {
  LiveFeedMode,
  LogEntry,
  LogSource,
  TerminalHeight,
  TerminalTab,
  WalletStatus,
} from "../../types";

const MAX_LOGS = 500;
const LS_KEY = "terminal-state";
const LOGS_LS_KEY = "terminal-logs";

type TerminalUnreadSource = "copytrade" | "alert";

interface TerminalUnread {
  copytrade: number;
  alert: number;
}

// --- State ---

interface TerminalState {
  isOpen: boolean;
  height: TerminalHeight;
  activeTab: TerminalTab;
  logs: LogEntry[];
  walletStatus: WalletStatus;
  activeSessions: number;
  walletBalance: string | null;
  unread: TerminalUnread;
  logsJumpNonce: number;
  detachedTabs: TerminalTab[];
  minimizedPanels: TerminalTab[];
  liveFeedMode: LiveFeedMode;
  liveFeedListId: string | null;
}

// --- Actions ---

type TerminalAction =
  | { type: "TOGGLE" }
  | { type: "SET_HEIGHT"; height: TerminalHeight }
  | { type: "SET_TAB"; tab: TerminalTab }
  | { type: "ADD_LOG"; entry: LogEntry }
  | { type: "CLEAR_LOGS" }
  | { type: "INCR_UNREAD"; source: TerminalUnreadSource; by?: number }
  | { type: "CLEAR_UNREAD"; source: TerminalUnreadSource }
  | { type: "CLEAR_ALL_UNREAD" }
  | { type: "TRIGGER_LOGS_JUMP" }
  | { type: "SET_WALLET_STATUS"; status: WalletStatus }
  | { type: "SET_ACTIVE_SESSIONS"; count: number }
  | { type: "SET_WALLET_BALANCE"; balance: string | null }
  | { type: "DETACH_TAB"; tab: TerminalTab }
  | { type: "ATTACH_TAB"; tab: TerminalTab }
  | { type: "MINIMIZE_PANEL"; tab: TerminalTab }
  | { type: "RESTORE_PANEL"; tab: TerminalTab }
  | { type: "SET_LIVE_FEED_MODE"; mode: LiveFeedMode }
  | { type: "SET_LIVE_FEED_LIST_ID"; listId: string | null };

function reducer(state: TerminalState, action: TerminalAction): TerminalState {
  switch (action.type) {
    case "TOGGLE": {
      const isOpen = !state.isOpen;
      return {
        ...state,
        isOpen,
        height: isOpen ? (state.height === "collapsed" ? "half" : state.height) : "collapsed",
      };
    }
    case "SET_HEIGHT": {
      const isOpen = action.height !== "collapsed";
      return { ...state, height: action.height, isOpen };
    }
    case "SET_TAB":
      return { ...state, activeTab: action.tab };
    case "ADD_LOG": {
      const logs =
        state.logs.length >= MAX_LOGS
          ? [...state.logs.slice(state.logs.length - MAX_LOGS + 1), action.entry]
          : [...state.logs, action.entry];
      return { ...state, logs };
    }
    case "CLEAR_LOGS":
      return { ...state, logs: [] };
    case "INCR_UNREAD": {
      const amount = Math.max(1, action.by ?? 1);
      return {
        ...state,
        unread: {
          ...state.unread,
          [action.source]: state.unread[action.source] + amount,
        },
      };
    }
    case "CLEAR_UNREAD":
      if (state.unread[action.source] === 0) return state;
      return {
        ...state,
        unread: {
          ...state.unread,
          [action.source]: 0,
        },
      };
    case "CLEAR_ALL_UNREAD":
      if (state.unread.copytrade === 0 && state.unread.alert === 0) return state;
      return {
        ...state,
        unread: { copytrade: 0, alert: 0 },
      };
    case "TRIGGER_LOGS_JUMP":
      return {
        ...state,
        logsJumpNonce: state.logsJumpNonce + 1,
      };
    case "SET_WALLET_STATUS":
      return { ...state, walletStatus: action.status };
    case "SET_ACTIVE_SESSIONS":
      return { ...state, activeSessions: action.count };
    case "SET_WALLET_BALANCE":
      return { ...state, walletBalance: action.balance };
    case "DETACH_TAB":
      if (state.detachedTabs.includes(action.tab)) return state;
      return { ...state, detachedTabs: [...state.detachedTabs, action.tab] };
    case "ATTACH_TAB":
      if (!state.detachedTabs.includes(action.tab)) return state;
      return {
        ...state,
        detachedTabs: state.detachedTabs.filter((t) => t !== action.tab),
        minimizedPanels: state.minimizedPanels.filter((t) => t !== action.tab),
      };
    case "MINIMIZE_PANEL":
      if (state.minimizedPanels.includes(action.tab)) return state;
      return { ...state, minimizedPanels: [...state.minimizedPanels, action.tab] };
    case "RESTORE_PANEL":
      if (!state.minimizedPanels.includes(action.tab)) return state;
      return { ...state, minimizedPanels: state.minimizedPanels.filter((t) => t !== action.tab) };
    case "SET_LIVE_FEED_MODE":
      return { ...state, liveFeedMode: action.mode };
    case "SET_LIVE_FEED_LIST_ID":
      return { ...state, liveFeedListId: action.listId };
  }
}

// --- Persisted initial state ---

interface PersistedTerminalState {
  height: TerminalHeight;
  activeTab: TerminalTab;
  detachedTabs: TerminalTab[];
  minimizedPanels: TerminalTab[];
  liveFeedMode: LiveFeedMode;
  liveFeedListId: string | null;
}

function loadPersistedState(): PersistedTerminalState {
  const defaults: PersistedTerminalState = {
    height: "collapsed",
    activeTab: "logs",
    detachedTabs: [],
    minimizedPanels: [],
    liveFeedMode: "public",
    liveFeedListId: null,
  };
  try {
    const raw = localStorage.getItem(LS_KEY);
    if (raw) {
      const parsed = JSON.parse(raw);
      return {
        height: parsed.height ?? defaults.height,
        activeTab: parsed.activeTab ?? defaults.activeTab,
        detachedTabs: Array.isArray(parsed.detachedTabs) ? parsed.detachedTabs : defaults.detachedTabs,
        minimizedPanels: Array.isArray(parsed.minimizedPanels) ? parsed.minimizedPanels : defaults.minimizedPanels,
        liveFeedMode: parsed.liveFeedMode === "signals" ? "signals" : defaults.liveFeedMode,
        liveFeedListId: parsed.liveFeedListId ?? defaults.liveFeedListId,
      };
    }
  } catch {
    // ignore
  }
  return defaults;
}

function persistState(s: Pick<TerminalState, "height" | "activeTab" | "detachedTabs" | "minimizedPanels" | "liveFeedMode" | "liveFeedListId">) {
  try {
    localStorage.setItem(LS_KEY, JSON.stringify(s));
  } catch {
    // ignore
  }
}

// --- Persisted logs ---

function loadPersistedLogs(): LogEntry[] {
  try {
    const raw = localStorage.getItem(LOGS_LS_KEY);
    if (!raw) return [];
    const parsed = JSON.parse(raw);
    if (!Array.isArray(parsed)) return [];
    return parsed
      .filter(
        (e: unknown): e is LogEntry =>
          !!e && typeof e === "object" && "id" in e && "timestamp" in e,
      )
      .slice(-MAX_LOGS)
      .map((entry) => ({
        ...entry,
        source: entry.source ?? ("wallet" as const),
      }));
  } catch {
    return [];
  }
}

// --- Contexts (split to avoid re-renders) ---

interface TerminalStateContextValue extends TerminalState {}

interface TerminalDispatchContextValue {
  toggle: () => void;
  setHeight: (h: TerminalHeight) => void;
  setActiveTab: (tab: TerminalTab) => void;
  incrUnread: (source: TerminalUnreadSource, by?: number) => void;
  clearUnread: (source: TerminalUnreadSource) => void;
  clearAllUnread: () => void;
  openLogsAndJumpToLatest: (source?: TerminalUnreadSource) => void;
  addLog: (level: LogEntry["level"], message: string, meta?: Record<string, string>, source?: LogSource) => void;
  clearLogs: () => void;
  setWalletStatus: (status: WalletStatus) => void;
  setActiveSessions: (count: number) => void;
  setWalletBalance: (balance: string | null) => void;
  detachTab: (tab: TerminalTab) => void;
  attachTab: (tab: TerminalTab) => void;
  minimizePanel: (tab: TerminalTab) => void;
  restorePanel: (tab: TerminalTab) => void;
  setLiveFeedMode: (mode: LiveFeedMode) => void;
  setLiveFeedListId: (listId: string | null) => void;
}

const TerminalStateContext = createContext<TerminalStateContextValue | null>(null);
const TerminalDispatchContext = createContext<TerminalDispatchContextValue | null>(null);

// --- Provider ---

export function TerminalProvider({ children }: { children: ReactNode }) {
  const persisted = useMemo(loadPersistedState, []);
  const persistedLogs = useMemo(loadPersistedLogs, []);

  const [state, dispatch] = useReducer(reducer, {
    isOpen: persisted.height !== "collapsed",
    height: persisted.height,
    activeTab: persisted.activeTab,
    logs: persistedLogs,
    walletStatus: "none",
    activeSessions: 0,
    walletBalance: null,
    unread: { copytrade: 0, alert: 0 },
    logsJumpNonce: 0,
    detachedTabs: persisted.detachedTabs,
    minimizedPanels: persisted.minimizedPanels,
    liveFeedMode: persisted.liveFeedMode,
    liveFeedListId: persisted.liveFeedListId,
  });

  // Persist state changes
  useEffect(() => {
    persistState({
      height: state.height,
      activeTab: state.activeTab,
      detachedTabs: state.detachedTabs,
      minimizedPanels: state.minimizedPanels,
      liveFeedMode: state.liveFeedMode,
      liveFeedListId: state.liveFeedListId,
    });
  }, [state.height, state.activeTab, state.detachedTabs, state.minimizedPanels, state.liveFeedMode, state.liveFeedListId]);

  // Persist logs (debounced 2s)
  useEffect(() => {
    const timer = setTimeout(() => {
      try {
        localStorage.setItem(LOGS_LS_KEY, JSON.stringify(state.logs));
      } catch {
        // storage full — silently fail
      }
    }, 2000);
    return () => clearTimeout(timer);
  }, [state.logs]);

  // Keyboard shortcuts
  useEffect(() => {
    const isMac = /Mac|iPhone|iPad/.test(navigator.userAgent);

    function isTypingTarget(target: EventTarget | null): boolean {
      if (!(target instanceof HTMLElement)) return false;
      const tag = target.tagName;
      return tag === "INPUT" || tag === "TEXTAREA" || target.isContentEditable;
    }

    function handleKeyDown(e: KeyboardEvent) {
      if (isTypingTarget(e.target)) return;

      // Ctrl+` toggle
      if (e.ctrlKey && e.key === "`") {
        e.preventDefault();
        dispatch({ type: "TOGGLE" });
        return;
      }

      // Alt+1..6 tab shortcuts (opens terminal if collapsed)
      // Alt+Shift+5/6 toggle detach for feed/alerts
      if (e.altKey && !e.metaKey && !e.ctrlKey) {
        if (e.shiftKey && (e.key === "5" || e.key === "%")) {
          e.preventDefault();
          dispatch(state.detachedTabs.includes("feed") ? { type: "ATTACH_TAB", tab: "feed" } : { type: "DETACH_TAB", tab: "feed" });
          return;
        }
        if (e.shiftKey && (e.key === "6" || e.key === "^")) {
          e.preventDefault();
          dispatch(state.detachedTabs.includes("alerts") ? { type: "ATTACH_TAB", tab: "alerts" } : { type: "DETACH_TAB", tab: "alerts" });
          return;
        }

        const tabByKey: Record<string, TerminalTab> = {
          "1": "wallet",
          "2": "sessions",
          "3": "logs",
          "4": "orders",
          "5": "feed",
          "6": "alerts",
        };
        const tab = tabByKey[e.key];
        if (tab) {
          e.preventDefault();
          dispatch({ type: "SET_HEIGHT", height: "half" });
          dispatch({ type: "SET_TAB", tab });
          return;
        }
      }

      // Cmd/Ctrl + Shift + ArrowUp/ArrowDown resize shortcuts
      const modifierPressed = isMac ? e.metaKey : e.ctrlKey;
      if (modifierPressed && e.shiftKey) {
        if (e.key === "ArrowUp" && state.isOpen) {
          e.preventDefault();
          dispatch({ type: "SET_HEIGHT", height: "full" });
          return;
        }
        if (e.key === "ArrowDown" && state.isOpen) {
          e.preventDefault();
          dispatch({ type: "SET_HEIGHT", height: "half" });
          return;
        }
      }

      // Escape collapse (only when terminal is open)
      if (e.key === "Escape" && state.isOpen) {
        dispatch({ type: "SET_HEIGHT", height: "collapsed" });
      }
    }
    window.addEventListener("keydown", handleKeyDown);
    return () => window.removeEventListener("keydown", handleKeyDown);
  }, [state.isOpen, state.detachedTabs]);

  const dispatchers = useMemo<TerminalDispatchContextValue>(
    () => ({
      toggle: () => dispatch({ type: "TOGGLE" }),
      setHeight: (height) => dispatch({ type: "SET_HEIGHT", height }),
      setActiveTab: (tab) => dispatch({ type: "SET_TAB", tab }),
      incrUnread: (source, by) => dispatch({ type: "INCR_UNREAD", source, by }),
      clearUnread: (source) => dispatch({ type: "CLEAR_UNREAD", source }),
      clearAllUnread: () => dispatch({ type: "CLEAR_ALL_UNREAD" }),
      openLogsAndJumpToLatest: (source) => {
        dispatch({ type: "SET_HEIGHT", height: "half" });
        dispatch({ type: "SET_TAB", tab: "logs" });
        if (source) {
          dispatch({ type: "CLEAR_UNREAD", source });
        }
        dispatch({ type: "TRIGGER_LOGS_JUMP" });
      },
      addLog: (level, message, meta, source = "wallet") =>
        dispatch({
          type: "ADD_LOG",
          entry: { id: crypto.randomUUID(), timestamp: Date.now(), level, source, message, meta },
        }),
      clearLogs: () => {
        dispatch({ type: "CLEAR_LOGS" });
        try { localStorage.removeItem(LOGS_LS_KEY); } catch {}
      },
      setWalletStatus: (status) => dispatch({ type: "SET_WALLET_STATUS", status }),
      setActiveSessions: (count) => dispatch({ type: "SET_ACTIVE_SESSIONS", count }),
      setWalletBalance: (balance) => dispatch({ type: "SET_WALLET_BALANCE", balance }),
      detachTab: (tab) => dispatch({ type: "DETACH_TAB", tab }),
      attachTab: (tab) => dispatch({ type: "ATTACH_TAB", tab }),
      minimizePanel: (tab) => dispatch({ type: "MINIMIZE_PANEL", tab }),
      restorePanel: (tab) => dispatch({ type: "RESTORE_PANEL", tab }),
      setLiveFeedMode: (mode) => dispatch({ type: "SET_LIVE_FEED_MODE", mode }),
      setLiveFeedListId: (listId) => dispatch({ type: "SET_LIVE_FEED_LIST_ID", listId }),
    }),
    [],
  );

  return (
    <TerminalStateContext.Provider value={state}>
      <TerminalDispatchContext.Provider value={dispatchers}>
        {children}
      </TerminalDispatchContext.Provider>
    </TerminalStateContext.Provider>
  );
}

// --- Hooks ---

export function useTerminalState() {
  const ctx = useContext(TerminalStateContext);
  if (!ctx) throw new Error("useTerminalState must be used within TerminalProvider");
  return ctx;
}

export function useTerminalDispatch() {
  const ctx = useContext(TerminalDispatchContext);
  if (!ctx) throw new Error("useTerminalDispatch must be used within TerminalProvider");
  return ctx;
}

export function useTerminal() {
  return { ...useTerminalState(), ...useTerminalDispatch() };
}
