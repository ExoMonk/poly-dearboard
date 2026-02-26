import { useState, useRef, useEffect } from "react";
import { motion, AnimatePresence } from "motion/react";
import EventActionChip, { actionLabel, type ActionKind } from "./EventActionChip";

export interface ActionDef {
  kind: ActionKind;
  onClick: (e: React.MouseEvent) => void;
  /** Render a custom component instead of chip (e.g. AddToListButton) */
  render?: React.ReactNode;
}

interface Props {
  actions: ActionDef[];
}

export default function EventActions({ actions }: Props) {
  if (actions.length === 0) return null;

  return (
    <>
      {/* Desktop: inline chips */}
      <div className="hidden sm:flex items-center gap-1">
        {actions.map((a) =>
          a.render ? (
            <span key={a.kind} onClick={(e) => e.stopPropagation()}>
              {a.render}
            </span>
          ) : (
            <EventActionChip key={a.kind} kind={a.kind} onClick={a.onClick} />
          ),
        )}
      </div>

      {/* Mobile: collapsed dropdown */}
      <div className="sm:hidden">
        <MobileActions actions={actions} />
      </div>
    </>
  );
}

function MobileActions({ actions }: { actions: ActionDef[] }) {
  const [open, setOpen] = useState(false);
  const ref = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!open) return;
    const handler = (e: MouseEvent) => {
      if (ref.current && !ref.current.contains(e.target as Node)) setOpen(false);
    };
    document.addEventListener("mousedown", handler);
    return () => document.removeEventListener("mousedown", handler);
  }, [open]);

  return (
    <div ref={ref} className="relative">
      <button
        onClick={(e) => { e.stopPropagation(); setOpen(!open); }}
        className="w-7 h-7 flex items-center justify-center rounded-md text-[var(--text-secondary)] hover:text-[var(--text-primary)] hover:bg-white/[0.06] transition-all text-sm"
        title="Actions"
      >
        ⋯
      </button>
      <AnimatePresence>
        {open && (
          <motion.div
            initial={{ opacity: 0, scale: 0.95, y: -4 }}
            animate={{ opacity: 1, scale: 1, y: 0 }}
            exit={{ opacity: 0, scale: 0.95, y: -4 }}
            transition={{ duration: 0.15 }}
            className="absolute right-0 top-full mt-1 z-50 min-w-[160px] py-1.5 rounded-lg bg-[var(--bg-card)] border border-[var(--border-glow)] shadow-xl"
          >
            {actions.map((a) =>
              a.render ? (
                <div key={a.kind} className="px-3 py-2" onClick={(e) => e.stopPropagation()}>
                  {a.render}
                </div>
              ) : (
                <button
                  key={a.kind}
                  onClick={(e) => { e.stopPropagation(); a.onClick(e); setOpen(false); }}
                  className="w-full text-left px-3 py-2 text-sm text-[var(--text-primary)] hover:bg-[var(--accent-blue)]/10 transition-colors cursor-pointer"
                >
                  {actionLabel(a.kind)}
                </button>
              ),
            )}
          </motion.div>
        )}
      </AnimatePresence>
    </div>
  );
}
