import { useEffect, useMemo, useRef, useState } from "react";
import { AnimatePresence, motion } from "motion/react";
import { useCommandRegistry } from "./useCommandRegistry";
import type { PaletteCommand } from "../../types";

const COMMAND_PALETTE_OPEN_EVENT = "terminal:open-command-palette";

function runFilter(commands: PaletteCommand[], query: string): PaletteCommand[] {
  const q = query.trim().toLowerCase();
  if (!q) return commands;

  const scored = commands
    .map((cmd) => {
      const haystack = `${cmd.label} ${(cmd.keywords ?? []).join(" ")}`.toLowerCase();
      const idx = haystack.indexOf(q);
      return { cmd, idx };
    })
    .filter((row) => row.idx >= 0)
    .sort((a, b) => a.idx - b.idx);

  return scored.map((row) => row.cmd);
}

export function CommandPalette() {
  const commands = useCommandRegistry();
  const [open, setOpen] = useState(false);
  const [query, setQuery] = useState("");
  const [activeIndex, setActiveIndex] = useState(0);
  const inputRef = useRef<HTMLInputElement>(null);

  const filtered = useMemo(() => runFilter(commands, query), [commands, query]);

  useEffect(() => {
    function isTypingTarget(target: EventTarget | null): boolean {
      if (!(target instanceof HTMLElement)) return false;
      const tag = target.tagName;
      return tag === "INPUT" || tag === "TEXTAREA" || target.isContentEditable;
    }

    function openPalette() {
      setOpen(true);
      try {
        localStorage.setItem("terminal-command-hint-seen", "1");
      } catch {
        // ignore storage errors
      }
    }

    function onOpenRequest() {
      openPalette();
    }

    function onKeyDown(e: KeyboardEvent) {
      const mod = /Mac|iPhone|iPad/.test(navigator.userAgent) ? e.metaKey : e.ctrlKey;
      if (mod && e.key.toLowerCase() === "k") {
        e.preventDefault();
        openPalette();
        return;
      }

      if (!open && !isTypingTarget(e.target) && !e.metaKey && !e.ctrlKey && !e.altKey) {
        if (e.key === "/" || e.key === ":") {
          e.preventDefault();
          openPalette();
          return;
        }
      }

      if (!open) return;

      if (e.key === "Escape") {
        e.preventDefault();
        e.stopPropagation();
        setOpen(false);
        return;
      }

      if (e.key === "Tab") {
        e.preventDefault();
        return;
      }

      if (e.key === "ArrowDown") {
        e.preventDefault();
        setActiveIndex((prev) => Math.min(prev + 1, Math.max(filtered.length - 1, 0)));
        return;
      }

      if (e.key === "ArrowUp") {
        e.preventDefault();
        setActiveIndex((prev) => Math.max(prev - 1, 0));
        return;
      }

      if (e.key === "Enter") {
        e.preventDefault();
        const selected = filtered[activeIndex];
        if (!selected) return;
        selected.action();
        setOpen(false);
      }
    }

    window.addEventListener("keydown", onKeyDown, true);
    window.addEventListener(COMMAND_PALETTE_OPEN_EVENT, onOpenRequest);
    return () => {
      window.removeEventListener("keydown", onKeyDown, true);
      window.removeEventListener(COMMAND_PALETTE_OPEN_EVENT, onOpenRequest);
    };
  }, [open, filtered, activeIndex]);

  useEffect(() => {
    if (!open) return;
    setActiveIndex(0);
    setTimeout(() => inputRef.current?.focus(), 0);
  }, [open]);

  useEffect(() => {
    setActiveIndex(0);
  }, [query]);

  useEffect(() => {
    if (!open) {
      document.body.style.overflow = "";
      return;
    }

    document.body.style.overflow = "hidden";
    return () => {
      document.body.style.overflow = "";
    };
  }, [open]);

  const closePalette = () => {
    setOpen(false);
    setQuery("");
    setActiveIndex(0);
  };

  return (
    <AnimatePresence>
      {open && (
        <motion.div
          className="fixed inset-0 z-50 bg-black/60 backdrop-blur-sm flex items-start justify-center pt-[12vh]"
          initial={{ opacity: 0 }}
          animate={{ opacity: 1 }}
          exit={{ opacity: 0 }}
          transition={{ duration: 0.1 }}
          role="dialog"
          aria-modal="true"
          onClick={closePalette}
        >
          <motion.div
            className="w-full max-w-[480px] rounded-xl border border-[var(--border-glow)] bg-[var(--bg-panel-solid)]/95 shadow-xl"
            initial={{ opacity: 0, y: -8 }}
            animate={{ opacity: 1, y: 0 }}
            exit={{ opacity: 0, y: -8 }}
            transition={{ duration: 0.1 }}
            onClick={(e) => e.stopPropagation()}
          >
            <div className="p-3 border-b border-white/10">
              <input
                ref={inputRef}
                role="combobox"
                aria-expanded="true"
                aria-controls="command-palette-list"
                aria-activedescendant={filtered[activeIndex] ? `command-option-${filtered[activeIndex].id}` : undefined}
                className="w-full bg-transparent text-sm text-[var(--text-primary)] placeholder:text-[var(--text-secondary)] outline-none"
                placeholder="Type a command..."
                value={query}
                onChange={(e) => setQuery(e.target.value)}
                onKeyDown={(e) => {
                  if (e.key === "Tab") e.preventDefault();
                }}
              />
            </div>

            <div
              id="command-palette-list"
              role="listbox"
              className="max-h-[288px] overflow-y-auto"
            >
              {filtered.length === 0 ? (
                <div className="px-3 py-3 text-xs text-[var(--text-muted)]">No matching commands.</div>
              ) : (
                filtered.map((cmd, index) => {
                  const active = index === activeIndex;
                  return (
                    <button
                      key={cmd.id}
                      id={`command-option-${cmd.id}`}
                      role="option"
                      aria-selected={active}
                      className={`w-full h-9 px-3 text-left text-xs flex items-center gap-2 ${
                        active ? "bg-[var(--accent-blue)]/15 text-[var(--text-primary)]" : "text-[var(--text-muted)] hover:bg-white/5"
                      }`}
                      onMouseEnter={() => setActiveIndex(index)}
                      onClick={() => {
                        cmd.action();
                        closePalette();
                      }}
                    >
                      <span className="min-w-0 truncate">{cmd.label}</span>
                      {cmd.shortcut && (
                        <span className="ml-auto font-mono text-[10px] text-[var(--text-secondary)]">{cmd.shortcut}</span>
                      )}
                    </button>
                  );
                })
              )}
            </div>
          </motion.div>
        </motion.div>
      )}
    </AnimatePresence>
  );
}

export function requestOpenCommandPalette() {
  window.dispatchEvent(new Event(COMMAND_PALETTE_OPEN_EVENT));
}
