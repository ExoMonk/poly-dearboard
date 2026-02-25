import { useMemo } from "react";
import { useNavigate } from "react-router-dom";
import { useSessions, useUpdateSession } from "../../hooks/useCopyTrade";
import { useTerminalDispatch, useTerminalState } from "./TerminalProvider";
import { requestOpenCreateSession } from "./CreateSessionModal";
import type { PaletteCommand } from "../../types";

const isMac = /Mac|iPhone|iPad/.test(navigator.userAgent);
const ALT = isMac ? "⌥" : "Alt+";
const CMD = isMac ? "⌘" : "Ctrl+";

function shortAddress(address: string): string {
  if (address.length <= 12) return address;
  return `${address.slice(0, 6)}...${address.slice(-4)}`;
}

export function useCommandRegistry() {
  const navigate = useNavigate();
  const { data: sessions } = useSessions();
  const updateSession = useUpdateSession();
  const { logs } = useTerminalState();
  const { toggle, setHeight, setActiveTab, clearLogs } = useTerminalDispatch();

  const staticCommands = useMemo<PaletteCommand[]>(() => [
    {
      id: "tab-wallet",
      label: "Switch to Wallet tab",
      section: "Terminal",
      shortcut: `${ALT}1`,
      keywords: ["wallet", "tab"],
      action: () => {
        setHeight("half");
        setActiveTab("wallet");
      },
    },
    {
      id: "tab-sessions",
      label: "Switch to Sessions tab",
      section: "Terminal",
      shortcut: `${ALT}2`,
      keywords: ["sessions", "tab", "copytrade"],
      action: () => {
        setHeight("half");
        setActiveTab("sessions");
      },
    },
    {
      id: "tab-logs",
      label: "Switch to Logs tab",
      section: "Terminal",
      shortcut: `${ALT}3`,
      keywords: ["logs", "terminal"],
      action: () => {
        setHeight("half");
        setActiveTab("logs");
      },
    },
    {
      id: "tab-orders",
      label: "Switch to Orders tab",
      section: "Terminal",
      shortcut: `${ALT}4`,
      keywords: ["orders", "tab"],
      action: () => {
        setHeight("half");
        setActiveTab("orders");
      },
    },
    {
      id: "terminal-toggle",
      label: "Toggle terminal",
      section: "Terminal",
      shortcut: "Ctrl+`",
      keywords: ["terminal", "toggle"],
      action: () => toggle(),
    },
    {
      id: "terminal-max",
      label: "Maximize terminal",
      section: "Terminal",
      shortcut: `${CMD}Shift+↑`,
      keywords: ["terminal", "maximize", "full"],
      action: () => setHeight("full"),
    },
    {
      id: "terminal-half",
      label: "Half-height terminal",
      section: "Terminal",
      shortcut: `${CMD}Shift+↓`,
      keywords: ["terminal", "half", "resize"],
      action: () => setHeight("half"),
    },
    {
      id: "clear-logs",
      label: "Clear logs",
      section: "Terminal",
      keywords: ["logs", "clear"],
      action: () => clearLogs(),
    },
    {
      id: "nav-dashboard",
      label: "Go to Dashboard",
      section: "Navigation",
      keywords: ["dashboard", "leaderboard", "home"],
      action: () => navigate("/"),
    },
    {
      id: "nav-activity",
      label: "Go to Activity",
      section: "Navigation",
      keywords: ["activity", "trades"],
      action: () => navigate("/activity"),
    },
    {
      id: "nav-alerts",
      label: "Go to Alerts",
      section: "Navigation",
      keywords: ["alerts", "whale", "resolution"],
      action: () => navigate("/alerts"),
    },
    {
      id: "nav-lab",
      label: "Go to Lab",
      section: "Navigation",
      keywords: ["lab", "signals", "backtest"],
      action: () => navigate("/lab"),
    },
    {
      id: "create-session",
      label: "Create new session",
      section: "Quick Actions",
      keywords: ["session", "copytrade", "start"],
      action: () => requestOpenCreateSession(),
    },
  ], [clearLogs, navigate, setActiveTab, setHeight, toggle]);

  const sessionCommands = useMemo<PaletteCommand[]>(() => {
    if (!sessions?.length) return [];

    const commands: PaletteCommand[] = [];
    for (const session of sessions) {
      const idShort = session.id.slice(0, 8);

      if (session.status === "running") {
        commands.push({
          id: `session-pause-${session.id}`,
          label: `Pause session ${idShort}`,
          section: "Session",
          keywords: ["pause", "session", idShort],
          action: () => updateSession.mutate({ id: session.id, action: "pause" }),
        });
      }

      if (session.status === "paused") {
        commands.push({
          id: `session-resume-${session.id}`,
          label: `Resume session ${idShort}`,
          section: "Session",
          keywords: ["resume", "session", idShort],
          action: () => updateSession.mutate({ id: session.id, action: "resume" }),
        });
      }

      if (session.status !== "stopped") {
        commands.push({
          id: `session-stop-${session.id}`,
          label: `Stop session ${idShort}`,
          section: "Session",
          keywords: ["stop", "session", idShort],
          action: () => updateSession.mutate({ id: session.id, action: "stop" }),
        });
      }
    }

    return commands;
  }, [sessions, updateSession]);

  const logJumpCommands = useMemo<PaletteCommand[]>(() => {
    const traderMap = new Map<string, string>();
    const marketMap = new Map<string, string>();

    for (let idx = logs.length - 1; idx >= 0; idx--) {
      const entry = logs[idx];
      const meta = entry.meta;
      if (!meta) continue;

      const trader = meta.trader ?? meta.source_trader;
      if (trader && !traderMap.has(trader) && traderMap.size < 5) {
        traderMap.set(trader, shortAddress(trader));
      }

      const assetId = meta.asset_id;
      if (assetId && !marketMap.has(assetId) && marketMap.size < 5) {
        marketMap.set(assetId, assetId.slice(0, 12));
      }

      if (traderMap.size >= 5 && marketMap.size >= 5) break;
    }

    const traderCommands: PaletteCommand[] = [...traderMap.entries()].map(([address, short]) => ({
      id: `jump-trader-${address}`,
      label: `Jump to trader ${short}`,
      section: "Quick Actions",
      keywords: ["trader", "jump", address],
      action: () => navigate(`/trader/${address}`),
    }));

    const marketCommands: PaletteCommand[] = [...marketMap.entries()].map(([tokenId, short]) => ({
      id: `jump-market-${tokenId}`,
      label: `Jump to market ${short}`,
      section: "Quick Actions",
      keywords: ["market", "jump", tokenId],
      action: () => navigate(`/market/${tokenId}`),
    }));

    return [...traderCommands, ...marketCommands];
  }, [logs, navigate]);

  return useMemo(
    () => [...staticCommands, ...sessionCommands, ...logJumpCommands],
    [staticCommands, sessionCommands, logJumpCommands],
  );
}
