import { useMemo, useState } from "react";
import { useNavigate } from "react-router-dom";
import type { PaletteCommand } from "../../types";
import { useCommandRegistry } from "./useCommandRegistry";
import { useTerminalDispatch } from "./TerminalProvider";

function shortAddress(address: string): string {
  if (address.length <= 12) return address;
  return `${address.slice(0, 6)}...${address.slice(-4)}`;
}

const HELP_COMMANDS = [
  "wallet",
  "sessions",
  "logs",
  "orders",
  "feed",
  "dashboard",
  "activity",
  "alerts",
  "lab",
  "max",
  "half",
  "toggle",
  "clear logs",
  "detach feed",
  "detach alerts",
  "attach feed",
  "attach alerts",
  "pause <session-id>",
  "resume <session-id>",
  "stop <session-id>",
  "trader <address>",
  "market <token-id>",
] as const;

const HELP_MESSAGE = `Available commands: ${HELP_COMMANDS.join(", ")}`;

function normalize(value: string): string {
  return value.trim().toLowerCase();
}

function isLikelyAddress(value: string): boolean {
  return /^0x[a-fA-F0-9]{40}$/.test(value.trim());
}

export function TerminalCommandPrompt() {
  const commands = useCommandRegistry();
  const { addLog } = useTerminalDispatch();
  const navigate = useNavigate();
  const [input, setInput] = useState("");
  const [history, setHistory] = useState<string[]>([]);
  const [, setHistoryIndex] = useState<number | null>(null);
  const [showCheatsheet, setShowCheatsheet] = useState(false);

  const suggestions = useMemo(() => {
    const query = normalize(input);
    if (!query) return commands.slice(0, 5);

    // Dynamic address jump suggestion
    const trimmed = input.trim();
    if (/^0x[a-fA-F0-9]{6,}$/i.test(trimmed)) {
      const addrSuggestion: PaletteCommand = {
        id: `dynamic-trader-${trimmed}`,
        label: `Jump to trader ${shortAddress(trimmed)}`,
        section: "Quick Actions",
        keywords: [],
        action: () => {
          navigate(`/trader/${trimmed}`);
          addLog("success", `Opened trader ${trimmed}`, undefined, "copytrade");
        },
      };
      return [addrSuggestion, ...commands
        .filter((cmd) => {
          const labelMatch = cmd.label.toLowerCase().includes(query);
          const keywordMatch = (cmd.keywords ?? []).some((keyword) => keyword.toLowerCase().includes(query));
          return labelMatch || keywordMatch;
        })
        .slice(0, 4)];
    }

    return commands
      .filter((cmd) => {
        const labelMatch = cmd.label.toLowerCase().includes(query);
        const keywordMatch = (cmd.keywords ?? []).some((keyword) => keyword.toLowerCase().includes(query));
        return labelMatch || keywordMatch;
      })
      .slice(0, 5);
  }, [commands, input, navigate, addLog]);

  function addToHistory(value: string) {
    setHistory((prev) => {
      if (prev[prev.length - 1] === value) return prev;
      const next = [...prev, value];
      return next.slice(-50);
    });
    setHistoryIndex(null);
  }

  function findCommandByAlias(raw: string) {
    const query = normalize(raw);

    const staticIdMap: Record<string, string> = {
      wallet: "tab-wallet",
      sessions: "tab-sessions",
      logs: "tab-logs",
      orders: "tab-orders",
      feed: "tab-feed",
      "live feed": "tab-feed",
      "livefeed": "tab-feed",
      dashboard: "nav-dashboard",
      activity: "nav-activity",
      alerts: "nav-alerts",
      lab: "nav-lab",
      "clear logs": "clear-logs",
      clear: "clear-logs",
      toggle: "terminal-toggle",
      "terminal toggle": "terminal-toggle",
      max: "terminal-max",
      "terminal max": "terminal-max",
      half: "terminal-half",
      "terminal half": "terminal-half",
      "tab wallet": "tab-wallet",
      "tab sessions": "tab-sessions",
      "tab logs": "tab-logs",
      "tab orders": "tab-orders",
      "tab feed": "tab-feed",
      "tab alerts": "tab-alerts",
      "detach feed": "detach-feed",
      "detach alerts": "detach-alerts",
      "attach feed": "attach-feed",
      "attach alerts": "attach-alerts",
      "go dashboard": "nav-dashboard",
      "go activity": "nav-activity",
      "go alerts": "nav-alerts",
      "go lab": "nav-lab",
    };

    const mappedId = staticIdMap[query];
    if (mappedId) {
      return commands.find((cmd) => cmd.id === mappedId);
    }

    const sessionMatch = query.match(/^(pause|resume|stop)\s+(.+)$/);
    if (sessionMatch) {
      const [, action, token] = sessionMatch;
      return commands.find((cmd) => {
        const label = cmd.label.toLowerCase();
        return label.startsWith(`${action} session`) && label.includes(token);
      });
    }

    return commands.find((cmd) => {
      const labelMatch = cmd.label.toLowerCase() === query;
      const keywordMatch = (cmd.keywords ?? []).some((keyword) => keyword.toLowerCase() === query);
      return labelMatch || keywordMatch;
    });
  }

  function runCommand(raw: string) {
    const query = normalize(raw);
    if (!query) return;

    // Bare hex address → navigate to trader
    if (isLikelyAddress(raw)) {
      navigate(`/trader/${raw.trim()}`);
      addToHistory(raw.trim());
      addLog("success", `Opened trader ${raw.trim()}`, undefined, "copytrade");
      setInput("");
      return;
    }

    const traderMatch = raw.trim().match(/^trader\s+(.+)$/i);
    if (traderMatch) {
      const address = traderMatch[1].trim();
      if (!isLikelyAddress(address)) {
        addLog("warn", `Invalid trader address: ${address}`, undefined, "copytrade");
        return;
      }
      const traderCommand = commands.find((cmd) => cmd.id === `jump-trader-${address}`);
      if (traderCommand) {
        traderCommand.action();
      } else {
        navigate(`/trader/${address}`);
      }
      addToHistory(raw.trim());
      addLog("success", `Opened trader ${address}`, undefined, "copytrade");
      setInput("");
      return;
    }

    const marketMatch = raw.trim().match(/^market\s+(.+)$/i);
    if (marketMatch) {
      const tokenId = marketMatch[1].trim();
      if (!tokenId) {
        addLog("warn", "Missing market token id", undefined, "copytrade");
        return;
      }
      const marketCommand = commands.find((cmd) => cmd.id === `jump-market-${tokenId}`);
      if (marketCommand) {
        marketCommand.action();
      } else {
        navigate(`/market/${tokenId}`);
      }
      addToHistory(raw.trim());
      addLog("success", `Opened market ${tokenId}`, undefined, "copytrade");
      setInput("");
      return;
    }

    if (query === "?") {
      setShowCheatsheet((prev) => !prev);
      setInput("");
      return;
    }

    if (query === "help") {
      addLog("info", HELP_MESSAGE, undefined, "copytrade");
      setShowCheatsheet(true);
      setInput("");
      return;
    }

    const exact = findCommandByAlias(query);
    if (exact) {
      exact.action();
      addLog("success", `Executed command: ${exact.label}`, undefined, "copytrade");
      addToHistory(raw.trim());
      setInput("");
      return;
    }

    const nearest = commands.find((cmd) => {
      const label = cmd.label.toLowerCase();
      const keywordText = (cmd.keywords ?? []).join(" ").toLowerCase();
      return label.includes(query) || keywordText.includes(query);
    });

    if (nearest) {
      nearest.action();
      addLog("info", `Executed closest match: ${nearest.label}`, undefined, "copytrade");
      addToHistory(raw.trim());
      setInput("");
      return;
    }

    addLog("warn", `Unknown command: ${raw.trim()}. Type 'help' to list common commands.`, undefined, "copytrade");
  }

  return (
    <div className="border-t border-white/5 bg-[var(--bg-panel)]/80 px-3 py-2">
      <div className="flex items-center gap-2 text-xs">
        <span className="font-mono text-[var(--accent-blue)]">&gt;</span>
        <input
          value={input}
          onChange={(e) => {
            setInput(e.target.value);
            setHistoryIndex(null);
          }}
          onKeyDown={(e) => {
            if (e.key === "?" && input.trim() === "") {
              e.preventDefault();
              setShowCheatsheet((prev) => !prev);
              return;
            }

            if (e.key === "Enter") {
              e.preventDefault();
              runCommand(input);
              return;
            }

            if (e.key === "ArrowUp") {
              e.preventDefault();
              setHistoryIndex((prev) => {
                const next = prev === null ? history.length - 1 : Math.max(0, prev - 1);
                if (next >= 0 && history[next]) setInput(history[next]);
                return next >= 0 ? next : null;
              });
              return;
            }

            if (e.key === "ArrowDown") {
              e.preventDefault();
              setHistoryIndex((prev) => {
                if (prev === null) return null;
                const next = prev + 1;
                if (next >= history.length) {
                  setInput("");
                  return null;
                }
                setInput(history[next] ?? "");
                return next;
              });
            }
          }}
          placeholder="Type command (help, trader <address>, market <token-id>)"
          className="flex-1 bg-transparent outline-none text-[var(--text-primary)] placeholder:text-[var(--text-muted)]"
        />
      </div>

      {showCheatsheet && (
        <div className="mt-1 rounded border border-white/10 bg-black/20 px-2 py-1.5">
          <div className="text-[10px] text-[var(--text-secondary)] mb-1">Cheatsheet (? to toggle)</div>
          <div className="flex flex-wrap gap-1.5">
            {HELP_COMMANDS.map((command) => (
              <button
                key={command}
                onClick={() => setInput(command.includes("<") ? command.replace(/<.*>/, "") : command)}
                className="text-[10px] px-1.5 py-0.5 rounded border border-white/10 text-[var(--text-secondary)] hover:bg-white/5 font-mono"
                title={`Use command: ${command}`}
              >
                {command}
              </button>
            ))}
          </div>
        </div>
      )}

      <div className="mt-1 flex flex-wrap gap-2">
        {suggestions.map((cmd) => (
          <button
            key={cmd.id}
            onClick={() => {
              cmd.action();
              addLog("success", `Executed command: ${cmd.label}`, undefined, "copytrade");
            }}
            className="text-[10px] px-1.5 py-0.5 rounded border border-white/10 text-[var(--text-secondary)] hover:bg-white/5"
            title={cmd.label}
          >
            {cmd.label}
          </button>
        ))}
      </div>
    </div>
  );
}