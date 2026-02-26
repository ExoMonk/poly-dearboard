import { createPortal } from "react-dom";
import { motion } from "motion/react";
import { useEffect, useRef, useState, useCallback, type ReactNode } from "react";

const LS_PREFIX = "detached-panel-";
const MIN_W = 320;
const MIN_H = 240;
const DEFAULT_W = 400;
const DEFAULT_H = 500;

interface Rect {
  x: number;
  y: number;
  w: number;
  h: number;
}

function clamp(r: Rect): Rect {
  const maxW = Math.min(r.w, window.innerWidth * 0.8);
  const maxH = Math.min(r.h, window.innerHeight * 0.8);
  const w = Math.max(MIN_W, maxW);
  const h = Math.max(MIN_H, maxH);
  const x = Math.max(0, Math.min(r.x, window.innerWidth - w));
  const y = Math.max(0, Math.min(r.y, window.innerHeight - h));
  return { x, y, w, h };
}

function loadRect(tabId: string): Rect {
  try {
    const raw = localStorage.getItem(LS_PREFIX + tabId);
    if (raw) {
      const p = JSON.parse(raw);
      return clamp({ x: p.x ?? 0, y: p.y ?? 0, w: p.w ?? DEFAULT_W, h: p.h ?? DEFAULT_H });
    }
  } catch { /* ignore */ }
  return clamp({ x: window.innerWidth - DEFAULT_W - 24, y: 24, w: DEFAULT_W, h: DEFAULT_H });
}

function saveRect(tabId: string, r: Rect) {
  try { localStorage.setItem(LS_PREFIX + tabId, JSON.stringify(r)); } catch { /* ignore */ }
}

interface Props {
  tabId: string;
  title: string;
  connected: boolean;
  minimized: boolean;
  onClose: () => void;
  onMinimize: () => void;
  onRestore: () => void;
  children: ReactNode;
}

export function DetachedPanel({ tabId, title, connected, minimized, onClose, onMinimize, onRestore, children }: Props) {
  const [rect, setRect] = useState<Rect>(() => loadRect(tabId));
  const dragging = useRef(false);
  const resizing = useRef(false);
  const origin = useRef({ mx: 0, my: 0, x: 0, y: 0, w: 0, h: 0 });
  const panelRef = useRef<HTMLDivElement>(null);

  // Persist rect changes (debounced)
  useEffect(() => {
    const t = setTimeout(() => saveRect(tabId, rect), 300);
    return () => clearTimeout(t);
  }, [tabId, rect]);

  // Recalculate bounds on resize
  useEffect(() => {
    function onResize() { setRect((r) => clamp(r)); }
    window.addEventListener("resize", onResize);
    return () => window.removeEventListener("resize", onResize);
  }, []);

  // Escape → minimize (stopPropagation so terminal doesn't collapse)
  useEffect(() => {
    function onKey(e: KeyboardEvent) {
      if (e.key === "Escape" && panelRef.current?.contains(document.activeElement)) {
        e.stopPropagation();
        e.preventDefault();
        if (minimized) onRestore();
        else onMinimize();
      }
    }
    window.addEventListener("keydown", onKey, true); // capture phase
    return () => window.removeEventListener("keydown", onKey, true);
  }, [minimized, onMinimize, onRestore]);

  // --- Drag ---
  const onDragStart = useCallback((e: React.PointerEvent) => {
    if ((e.target as HTMLElement).closest("button")) return; // Don't drag from buttons
    dragging.current = true;
    origin.current = { mx: e.clientX, my: e.clientY, x: rect.x, y: rect.y, w: rect.w, h: rect.h };
    (e.target as HTMLElement).setPointerCapture(e.pointerId);
    document.body.style.userSelect = "none";
  }, [rect]);

  const onDragMove = useCallback((e: React.PointerEvent) => {
    if (!dragging.current) return;
    const dx = e.clientX - origin.current.mx;
    const dy = e.clientY - origin.current.my;
    setRect((r) => clamp({ ...r, x: origin.current.x + dx, y: origin.current.y + dy }));
  }, []);

  const onDragEnd = useCallback(() => {
    dragging.current = false;
    document.body.style.userSelect = "";
  }, []);

  // --- Resize ---
  const onResizeStart = useCallback((e: React.PointerEvent) => {
    e.stopPropagation();
    resizing.current = true;
    origin.current = { mx: e.clientX, my: e.clientY, x: rect.x, y: rect.y, w: rect.w, h: rect.h };
    (e.target as HTMLElement).setPointerCapture(e.pointerId);
    document.body.style.userSelect = "none";
  }, [rect]);

  const onResizeMove = useCallback((e: React.PointerEvent) => {
    if (!resizing.current) return;
    const dw = e.clientX - origin.current.mx;
    const dh = e.clientY - origin.current.my;
    setRect(() => clamp({ x: origin.current.x, y: origin.current.y, w: origin.current.w + dw, h: origin.current.h + dh }));
  }, []);

  const onResizeEnd = useCallback(() => {
    resizing.current = false;
    document.body.style.userSelect = "";
  }, []);

  const panel = (
    <motion.div
      ref={panelRef}
      className="detached-panel fixed flex flex-col overflow-hidden"
      style={{ left: rect.x, top: rect.y, width: rect.w, height: minimized ? "auto" : rect.h, zIndex: 45 }}
      initial={{ opacity: 0, scale: 0.95 }}
      animate={{ opacity: 1, scale: 1 }}
      exit={{ opacity: 0, scale: 0.95 }}
      transition={{ duration: 0.15 }}
      tabIndex={-1}
    >
      {/* Header */}
      <div
        className="detached-panel-header flex items-center gap-2 px-3 py-1.5 select-none shrink-0"
        onPointerDown={onDragStart}
        onPointerMove={onDragMove}
        onPointerUp={onDragEnd}
        onDoubleClick={() => minimized ? onRestore() : onMinimize()}
      >
        {/* Connection dot */}
        <span className={`w-1.5 h-1.5 rounded-full shrink-0 ${connected ? "bg-green-500" : "bg-red-500"}`} />
        <span className="text-xs font-medium text-[var(--text-primary)] truncate flex-1">{title}</span>

        {/* Minimize */}
        <button
          onClick={(e) => { e.stopPropagation(); minimized ? onRestore() : onMinimize(); }}
          className="p-0.5 rounded hover:bg-white/10 text-[var(--text-muted)]"
          title={minimized ? "Restore" : "Minimize"}
        >
          <svg className="w-3.5 h-3.5" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
            {minimized ? <path d="M15 3h6v6M9 21H3v-6M21 3l-7 7M3 21l7-7" /> : <path d="M5 12h14" />}
          </svg>
        </button>

        {/* Close (re-attach) */}
        <button
          onClick={(e) => { e.stopPropagation(); onClose(); }}
          className="p-0.5 rounded hover:bg-white/10 text-[var(--text-muted)]"
          title="Re-attach to Terminal"
        >
          <svg className="w-3.5 h-3.5" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
            <path d="M18 6L6 18M6 6l12 12" />
          </svg>
        </button>
      </div>

      {/* Content */}
      {!minimized && (
        <div className="flex-1 min-h-0 overflow-hidden relative">
          {children}
          {/* Resize handle */}
          <div
            className="absolute bottom-0 right-0 w-4 h-4 cursor-nwse-resize z-10"
            onPointerDown={onResizeStart}
            onPointerMove={onResizeMove}
            onPointerUp={onResizeEnd}
          >
            <svg className="w-3 h-3 text-white/20 absolute bottom-0.5 right-0.5" viewBox="0 0 12 12" fill="currentColor">
              <circle cx="10" cy="10" r="1.5" />
              <circle cx="6" cy="10" r="1.5" />
              <circle cx="10" cy="6" r="1.5" />
            </svg>
          </div>
        </div>
      )}
    </motion.div>
  );

  return createPortal(panel, document.body);
}
