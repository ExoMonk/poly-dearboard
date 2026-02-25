import { useState, useMemo, useRef, useEffect } from "react";
import { Link } from "react-router-dom";
import { useSessions, useSessionOrders } from "../../hooks/useCopyTrade";
import type { CopyTradeOrder, OrderStatus } from "../../types";

type SortField = "created_at" | "side" | "size_usdc" | "price" | "slippage_bps" | "status";
type SortDir = "asc" | "desc";

const STATUS_COLORS: Record<OrderStatus, string> = {
  filled: "bg-green-500/20 text-green-400",
  failed: "bg-red-500/20 text-red-400",
  pending: "bg-yellow-500/20 text-yellow-400",
  submitted: "bg-yellow-500/20 text-yellow-400",
  canceled: "bg-neutral-500/20 text-neutral-400",
  simulated: "bg-blue-500/20 text-blue-400",
  partial: "bg-orange-500/20 text-orange-400",
};

const STATUS_OPTIONS: (OrderStatus | "all")[] = ["all", "filled", "partial", "failed", "pending", "canceled", "simulated"];
const SIDE_OPTIONS = ["all", "buy", "sell"] as const;
const PAGE_SIZE = 50;

function SortHeader({ label, field, sort, onSort }: {
  label: string;
  field: SortField;
  sort: { field: SortField; dir: SortDir };
  onSort: (f: SortField) => void;
}) {
  const active = sort.field === field;
  return (
    <th
      className="px-2 py-1.5 text-left cursor-pointer hover:text-[var(--text-primary)] select-none"
      onClick={() => onSort(field)}
    >
      {label} {active ? (sort.dir === "asc" ? "\u25B2" : "\u25BC") : ""}
    </th>
  );
}

const NEW_HIGHLIGHT: Record<string, string> = {
  filled: "animate-highlight-green",
  simulated: "animate-highlight-blue",
  pending: "animate-highlight-yellow",
  submitted: "animate-highlight-yellow",
  partial: "animate-highlight-orange",
  failed: "animate-highlight-red",
  default: "animate-highlight-green",
};

type IncomingLevel = "success" | "warn" | "error";

function incomingBannerStyle(level: IncomingLevel): string {
  switch (level) {
    case "error":
      return "bg-red-500/15 border-red-500/30 text-red-300";
    case "warn":
      return "bg-yellow-500/15 border-yellow-500/30 text-yellow-200";
    default:
      return "bg-green-500/15 border-green-500/30 text-green-300";
  }
}

function summarizeIncoming(orders: CopyTradeOrder[]): { message: string; level: IncomingLevel } {
  const failed = orders.filter((o) => o.status === "failed").length;
  const pendingLike = orders.filter((o) => o.status === "pending" || o.status === "submitted" || o.status === "partial").length;
  const filledLike = orders.filter((o) => o.status === "filled" || o.status === "simulated").length;

  if (failed > 0) {
    return {
      level: "error",
      message: `⚠ ${orders.length} new trade update${orders.length > 1 ? "s" : ""} — ${failed} failed`,
    };
  }

  if (pendingLike > 0) {
    return {
      level: "warn",
      message: `⚠ Incoming trade${pendingLike > 1 ? "s" : ""} — ${pendingLike} pending/submitted`,
    };
  }

  return {
    level: "success",
    message: `✓ ${filledLike} new filled/simulated trade${filledLike > 1 ? "s" : ""}`,
  };
}

function OrderRow({ order, expanded, onToggle, isNew }: {
  order: CopyTradeOrder;
  expanded: boolean;
  onToggle: () => void;
  isNew?: boolean;
}) {
  const time = new Date(order.created_at).toLocaleString(undefined, {
    month: "short", day: "numeric", hour: "2-digit", minute: "2-digit", second: "2-digit",
  });

  const highlightClass = isNew
    ? (NEW_HIGHLIGHT[order.status] ?? NEW_HIGHLIGHT.default)
    : "";

  return (
    <>
      <tr
        className={`border-b border-[var(--border-subtle)] hover:bg-[var(--surface-2)]/50 cursor-pointer text-xs ${highlightClass}`}
        onClick={onToggle}
      >
        <td className="px-2 py-1.5 font-mono text-[var(--text-muted)]">{time}</td>
        <td className="px-2 py-1.5">
          <span className={`font-mono font-semibold ${order.side === "buy" ? "text-green-400" : "text-red-400"}`}>
            {order.side.toUpperCase()}
          </span>
        </td>
        <td className="px-2 py-1.5 font-mono">${order.size_usdc.toFixed(2)}</td>
        <td className="px-2 py-1.5 font-mono">
          {order.fill_price != null ? `$${order.fill_price.toFixed(4)}` : `$${order.price.toFixed(4)}`}
        </td>
        <td className="px-2 py-1.5 font-mono">
          {order.slippage_bps != null ? `${order.slippage_bps.toFixed(1)}` : "\u2014"}
        </td>
        <td className="px-2 py-1.5">
          <span className={`px-1.5 py-0.5 rounded text-[10px] font-mono ${STATUS_COLORS[order.status]}`}>
            {order.status}
          </span>
        </td>
      </tr>
      {expanded && (
        <tr className="border-b border-[var(--border-subtle)] bg-[var(--surface-2)]/30">
          <td colSpan={6} className="px-3 py-2">
            <div className="grid grid-cols-2 gap-x-6 gap-y-1 text-xs">
              <div>
                <span className="text-[var(--text-muted)]">Source Trader: </span>
                <span className="font-mono">{order.source_trader.slice(0, 6)}...{order.source_trader.slice(-4)}</span>
              </div>
              <div>
                <span className="text-[var(--text-muted)]">Market: </span>
                <Link
                  to={`/market/${order.asset_id}`}
                  className="font-mono text-blue-400 hover:underline"
                  onClick={(e) => e.stopPropagation()}
                >
                  {order.asset_id.slice(0, 12)}...
                </Link>
              </div>
              {order.clob_order_id && (
                <div>
                  <span className="text-[var(--text-muted)]">CLOB Order: </span>
                  <span className="font-mono">{order.clob_order_id.slice(0, 12)}...</span>
                </div>
              )}
              {order.tx_hash && (
                <div>
                  <span className="text-[var(--text-muted)]">Tx: </span>
                  <a
                    href={`https://polygonscan.com/tx/${order.tx_hash}`}
                    target="_blank"
                    rel="noopener noreferrer"
                    className="font-mono text-blue-400 hover:underline"
                    onClick={(e) => e.stopPropagation()}
                  >
                    {order.tx_hash.slice(0, 10)}...
                  </a>
                </div>
              )}
              {order.error_message && (
                <div className="col-span-2">
                  <span className="text-[var(--text-muted)]">Error: </span>
                  <span className="font-mono text-red-400">{order.error_message}</span>
                </div>
              )}
              <div>
                <span className="text-[var(--text-muted)]">Source Price: </span>
                <span className="font-mono">${order.source_price.toFixed(4)}</span>
              </div>
              {order.size_shares != null && (
                <div>
                  <span className="text-[var(--text-muted)]">Shares: </span>
                  <span className="font-mono">{order.size_shares.toFixed(4)}</span>
                </div>
              )}
            </div>
          </td>
        </tr>
      )}
    </>
  );
}

export function OrdersTab() {
  const { data: sessions } = useSessions();
  const [selectedSession, setSelectedSession] = useState<string | null>(null);
  const [statusFilter, setStatusFilter] = useState<OrderStatus | "all">("all");
  const [sideFilter, setSideFilter] = useState<"all" | "buy" | "sell">("all");
  const [sort, setSort] = useState<{ field: SortField; dir: SortDir }>({ field: "created_at", dir: "desc" });
  const [expandedId, setExpandedId] = useState<string | null>(null);
  const [page, setPage] = useState(0);
  const [incomingNotice, setIncomingNotice] = useState<{ message: string; level: IncomingLevel } | null>(null);

  // Auto-select first session
  const sessionId = selectedSession ?? sessions?.[0]?.id ?? null;

  const { data: orders, isLoading } = useSessionOrders(sessionId, (page + 1) * PAGE_SIZE);

  // Track new orders for highlight animation
  const prevOrderIdsRef = useRef<Set<string>>(new Set());
  const [newOrderIds, setNewOrderIds] = useState<Set<string>>(new Set());

  useEffect(() => {
    if (!orders) return;
    const currentIds = new Set(orders.map((o) => o.id));
    const prev = prevOrderIdsRef.current;
    if (prev.size > 0) {
      const freshIds = new Set<string>();
      for (const id of currentIds) {
        if (!prev.has(id)) freshIds.add(id);
      }
      if (freshIds.size > 0) {
        const freshOrders = orders.filter((o) => freshIds.has(o.id));
        setIncomingNotice(summarizeIncoming(freshOrders));
        setNewOrderIds(freshIds);
        // Clear highlights/notice after animation window (3s)
        setTimeout(() => setNewOrderIds(new Set()), 3000);
        setTimeout(() => setIncomingNotice(null), 3000);
      }
    }
    prevOrderIdsRef.current = currentIds;
  }, [orders]);

  const filtered = useMemo(() => {
    if (!orders) return [];
    let result = orders;
    if (statusFilter !== "all") result = result.filter((o) => o.status === statusFilter);
    if (sideFilter !== "all") result = result.filter((o) => o.side === sideFilter);
    return result.sort((a, b) => {
      const dir = sort.dir === "asc" ? 1 : -1;
      const av = a[sort.field];
      const bv = b[sort.field];
      if (av == null && bv == null) return 0;
      if (av == null) return 1;
      if (bv == null) return -1;
      return av < bv ? -dir : av > bv ? dir : 0;
    });
  }, [orders, statusFilter, sideFilter, sort]);

  const handleSort = (field: SortField) => {
    setSort((prev) =>
      prev.field === field ? { field, dir: prev.dir === "asc" ? "desc" : "asc" } : { field, dir: "desc" }
    );
  };

  const pillCls = (active: boolean) =>
    `px-2 py-0.5 text-[10px] rounded border cursor-pointer ${
      active
        ? "bg-[var(--neon-green)]/10 text-[var(--neon-green)] border-[var(--neon-green)]/30"
        : "bg-[var(--surface-2)] text-[var(--text-muted)] border-[var(--border-subtle)] hover:text-[var(--text-primary)]"
    }`;

  if (!sessions || sessions.length === 0) {
    return (
      <div className="flex items-center justify-center h-full text-xs text-[var(--text-muted)]">
        No sessions — create one in the Sessions tab.
      </div>
    );
  }

  return (
    <div className="flex flex-col h-full">
      {/* Toolbar */}
      <div className="flex items-center gap-3 px-3 py-2 border-b border-[var(--border-subtle)] flex-shrink-0">
        {/* Session selector */}
        <select
          className="bg-[var(--surface-2)] border border-[var(--border-subtle)] rounded px-2 py-0.5 text-xs font-mono text-[var(--text-primary)] focus:outline-none"
          value={sessionId ?? ""}
          onChange={(e) => { setSelectedSession(e.target.value); setPage(0); }}
        >
          {sessions.map((s) => (
            <option key={s.id} value={s.id}>
              {s.id.slice(0, 8)} ({s.status}{s.simulate ? " SIM" : ""})
            </option>
          ))}
        </select>

        {/* Status filters */}
        <div className="flex gap-1">
          {STATUS_OPTIONS.map((s) => (
            <button key={s} className={pillCls(statusFilter === s)} onClick={() => { setStatusFilter(s); setPage(0); }}>
              {s === "all" ? "All" : s}
            </button>
          ))}
        </div>

        {/* Side filters */}
        <div className="flex gap-1 ml-1">
          {SIDE_OPTIONS.map((s) => (
            <button key={s} className={pillCls(sideFilter === s)} onClick={() => { setSideFilter(s); setPage(0); }}>
              {s === "all" ? "All" : s.toUpperCase()}
            </button>
          ))}
        </div>
      </div>

      {incomingNotice && (
        <div className={`mx-3 mt-2 px-2.5 py-1.5 rounded border text-xs font-mono ${incomingBannerStyle(incomingNotice.level)}`}>
          {incomingNotice.message}
        </div>
      )}

      {/* Table */}
      <div className="flex-1 overflow-auto min-h-0">
        {isLoading ? (
          <div className="flex items-center justify-center h-full text-xs text-[var(--text-muted)]">Loading orders...</div>
        ) : filtered.length === 0 ? (
          <div className="flex items-center justify-center h-full text-xs text-[var(--text-muted)]">No orders match filters.</div>
        ) : (
          <table className="w-full text-xs">
            <thead className="text-[10px] text-[var(--text-muted)] uppercase tracking-wider sticky top-0 bg-[var(--surface-1)]">
              <tr>
                <SortHeader label="Time" field="created_at" sort={sort} onSort={handleSort} />
                <SortHeader label="Side" field="side" sort={sort} onSort={handleSort} />
                <SortHeader label="Size" field="size_usdc" sort={sort} onSort={handleSort} />
                <SortHeader label="Price" field="price" sort={sort} onSort={handleSort} />
                <SortHeader label="Slip (bps)" field="slippage_bps" sort={sort} onSort={handleSort} />
                <SortHeader label="Status" field="status" sort={sort} onSort={handleSort} />
              </tr>
            </thead>
            <tbody>
              {filtered.map((order) => (
                <OrderRow
                  key={order.id}
                  order={order}
                  expanded={expandedId === order.id}
                  onToggle={() => setExpandedId(expandedId === order.id ? null : order.id)}
                  isNew={newOrderIds.has(order.id)}
                />
              ))}
            </tbody>
          </table>
        )}
      </div>

      {/* Load more */}
      {orders && orders.length >= (page + 1) * PAGE_SIZE && (
        <div className="flex justify-center py-1.5 border-t border-[var(--border-subtle)] flex-shrink-0">
          <button
            className="px-3 py-0.5 text-xs text-[var(--text-muted)] hover:text-[var(--text-primary)]"
            onClick={() => setPage((p) => p + 1)}
          >
            Load more...
          </button>
        </div>
      )}
    </div>
  );
}
