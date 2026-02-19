import { useState, useEffect, useMemo } from "react";
import {
  AreaChart,
  Area,
  XAxis,
  YAxis,
  Tooltip,
  ResponsiveContainer,
  CartesianGrid,
  ReferenceLine,
} from "recharts";
import { motion } from "motion/react";
import type { PricePoint, MarketWsStatus, BidAsk } from "../types";
import { panelVariants } from "../lib/motion";

interface Props {
  priceHistory: PricePoint[];
  tradeMarkers: Set<number>;
  status: MarketWsStatus;
  bidAsk: BidAsk;
}

const WINDOWS = { "1m": 60_000, "5m": 300_000 } as const;
type TimeWindow = keyof typeof WINDOWS;

const TOOLTIP_STYLE = {
  backgroundColor: "rgba(10, 18, 40, 0.95)",
  border: "1px solid rgba(59, 130, 246, 0.2)",
  borderRadius: 8,
  fontSize: 12,
  boxShadow: "0 4px 20px rgba(0, 0, 0, 0.5)",
  padding: "10px 14px",
};

interface ChartDatum {
  timestamp: number;
  yes: number;
  no: number;
  isOurTrade: boolean;
}

function formatTick(ts: number): string {
  const d = new Date(ts);
  return d.toLocaleTimeString("en-US", {
    minute: "2-digit",
    second: "2-digit",
    hour12: false,
  });
}

function CustomTooltip({
  active,
  payload,
}: {
  active?: boolean;
  payload?: Array<{ payload: ChartDatum }>;
}) {
  if (!active || !payload?.length) return null;
  const d = payload[0].payload;

  return (
    <div style={TOOLTIP_STYLE}>
      <div
        style={{
          color: "var(--accent-blue)",
          marginBottom: 6,
          fontSize: 11,
          fontFamily: "monospace",
        }}
      >
        {new Date(d.timestamp).toLocaleTimeString("en-US", {
          hour: "2-digit",
          minute: "2-digit",
          second: "2-digit",
          hour12: false,
        })}
      </div>
      <div
        style={{
          display: "grid",
          gridTemplateColumns: "auto auto",
          gap: "2px 12px",
        }}
      >
        <span style={{ color: "var(--neon-green)" }}>Yes</span>
        <span style={{ color: "var(--neon-green)", fontFamily: "monospace" }}>
          {d.yes.toFixed(1)}&cent;
        </span>
        <span style={{ color: "var(--neon-red)" }}>No</span>
        <span style={{ color: "var(--neon-red)", fontFamily: "monospace" }}>
          {d.no.toFixed(1)}&cent;
        </span>
      </div>
      {d.isOurTrade && (
        <div style={{ color: "var(--accent-blue)", fontSize: 10, marginTop: 4 }}>
          Indexed trade (on-chain)
        </div>
      )}
    </div>
  );
}

/** Large ring dot for our indexed on-chain trades */
function TradeDot(props: { cx?: number; cy?: number; payload?: ChartDatum }) {
  const { cx, cy, payload } = props;
  if (!payload?.isOurTrade || cx == null || cy == null) return null;
  return (
    <g>
      {/* Outer glow */}
      <circle cx={cx} cy={cy} r={10} fill="rgba(59, 130, 246, 0.15)" />
      {/* Ring */}
      <circle
        cx={cx}
        cy={cy}
        r={6}
        fill="var(--accent-blue)"
        stroke="white"
        strokeWidth={2}
        fillOpacity={0.9}
      />
    </g>
  );
}

export default function LivePriceChart({
  priceHistory,
  tradeMarkers,
  status,
  bidAsk,
}: Props) {
  const [timeWindow, setTimeWindow] = useState<TimeWindow>("5m");
  const [tick, setTick] = useState(Date.now());

  // Tick every second so the sliding window moves in real time
  useEffect(() => {
    const id = setInterval(() => setTick(Date.now()), 1000);
    return () => clearInterval(id);
  }, []);

  const windowMs = WINDOWS[timeWindow];
  const windowStart = tick - windowMs;

  // Build chart data: filter to visible window + extend to "now"
  const { data, yDomain, currentYes } = useMemo(() => {
    // Convert to chart coordinates (cents) and filter to window
    const visible: ChartDatum[] = [];
    let lastBeforeWindow: ChartDatum | null = null;

    for (const p of priceHistory) {
      const datum: ChartDatum = {
        timestamp: p.timestamp,
        yes: Math.round(p.yesPrice * 10000) / 100,
        no: Math.round(p.noPrice * 10000) / 100,
        isOurTrade: tradeMarkers.has(p.timestamp),
      };

      if (p.timestamp < windowStart) {
        // Keep track of the last point before window for continuity
        lastBeforeWindow = datum;
      } else {
        visible.push(datum);
      }
    }

    // Prepend the last-before-window point at the window edge for line continuity
    if (lastBeforeWindow && visible.length > 0) {
      visible.unshift({
        ...lastBeforeWindow,
        timestamp: windowStart,
        isOurTrade: false,
      });
    }

    // Deduplicate by timestamp — keep last value, preserve trade markers
    const deduped = new Map<number, ChartDatum>();
    for (const d of visible) {
      const existing = deduped.get(d.timestamp);
      deduped.set(d.timestamp, existing?.isOurTrade && !d.isOurTrade
        ? { ...d, isOurTrade: true }
        : d,
      );
    }
    visible.length = 0;
    visible.push(...deduped.values());

    // Extend to "now" so line reaches the right edge
    if (visible.length > 0) {
      const last = visible[visible.length - 1];
      if (tick - last.timestamp > 1000) {
        visible.push({
          timestamp: tick,
          yes: last.yes,
          no: last.no,
          isOurTrade: false,
        });
      }
    }

    // Compute Y domain centered on Yes price with good padding
    const yesValues = visible.map((d) => d.yes);
    const curYes = yesValues.length > 0 ? yesValues[yesValues.length - 1] : 50;
    const yMin = yesValues.length > 0 ? Math.min(...yesValues) : 40;
    const yMax = yesValues.length > 0 ? Math.max(...yesValues) : 60;
    const range = yMax - yMin;

    // Ensure minimum 10¢ visible range, centered on the data
    const minRange = 10;
    const pad = Math.max(2, (Math.max(range, minRange) - range) / 2 + 2);
    const domainMin = Math.max(0, Math.floor(yMin - pad));
    const domainMax = Math.min(100, Math.ceil(yMax + pad));

    return {
      data: visible,
      yDomain: [domainMin, domainMax] as [number, number],
      currentYes: curYes,
    };
  }, [priceHistory, tradeMarkers, windowStart, tick]);

  return (
    <motion.div
      variants={panelVariants}
      initial="initial"
      animate="animate"
      transition={{ duration: 0.4 }}
      className="glass p-5 gradient-border-top"
    >
      {/* Header */}
      <div className="flex items-center justify-between mb-4">
        <div className="flex items-center gap-3">
          <h3 className="text-sm font-medium text-[var(--text-secondary)] uppercase tracking-wider">
            Price
          </h3>
          {data.length >= 2 && (
            <span className="text-lg font-bold font-mono text-[var(--neon-green)]">
              {currentYes.toFixed(1)}&cent;
            </span>
          )}
        </div>
        <div className="flex items-center gap-3">
          {/* Time window selector */}
          <div className="flex gap-1">
            {(Object.keys(WINDOWS) as TimeWindow[]).map((w) => (
              <button
                key={w}
                onClick={() => setTimeWindow(w)}
                className={`px-2.5 py-1 text-xs font-mono rounded transition-colors ${
                  timeWindow === w
                    ? "bg-[var(--accent-blue)]/20 text-[var(--accent-blue)] border border-[var(--accent-blue)]/30"
                    : "text-[var(--text-secondary)] hover:text-[var(--text-primary)]"
                }`}
              >
                {w}
              </button>
            ))}
          </div>
          {bidAsk.spread != null && (
            <span className="text-xs font-mono text-[var(--text-secondary)]">
              Spread: {(bidAsk.spread * 100).toFixed(1)}&cent;
            </span>
          )}
          <span
            className={`flex items-center gap-1.5 text-xs ${
              status === "connected"
                ? "text-[var(--neon-green)]"
                : "text-[var(--text-secondary)]"
            }`}
          >
            <span
              className={`w-1.5 h-1.5 rounded-full ${
                status === "connected"
                  ? "bg-[var(--neon-green)] neon-pulse shadow-[0_0_6px_var(--neon-green)]"
                  : status === "connecting"
                    ? "bg-[var(--accent-orange)] animate-pulse"
                    : "bg-[var(--neon-red)]"
              }`}
            />
            {status === "connected"
              ? "Polymarket Live"
              : status === "connecting"
                ? "Connecting"
                : "Offline"}
          </span>
        </div>
      </div>

      {/* Chart */}
      {data.length < 2 ? (
        <p className="text-[var(--text-secondary)] text-center py-16 text-sm">
          Waiting for price data...
        </p>
      ) : (
        <ResponsiveContainer width="100%" height={280}>
          <AreaChart
            data={data}
            margin={{ left: 10, right: 10, top: 10, bottom: 0 }}
          >
            <defs>
              <linearGradient id="yesFill" x1="0" y1="0" x2="0" y2="1">
                <stop offset="0%" stopColor="#00ff88" stopOpacity={0.15} />
                <stop offset="100%" stopColor="#00ff88" stopOpacity={0.02} />
              </linearGradient>
            </defs>
            <CartesianGrid
              strokeDasharray="3 3"
              stroke="rgba(59, 130, 246, 0.06)"
              vertical={false}
            />
            <XAxis
              dataKey="timestamp"
              type="number"
              domain={[windowStart, tick]}
              scale="time"
              tick={{ fill: "var(--text-secondary)", fontSize: 10 }}
              tickFormatter={formatTick}
              axisLine={false}
              tickLine={false}
            />
            <YAxis
              domain={yDomain}
              tick={{ fill: "var(--text-secondary)", fontSize: 10 }}
              tickFormatter={(v: number) => `${v}\u00a2`}
              axisLine={false}
              tickLine={false}
              width={40}
            />
            <Tooltip content={<CustomTooltip />} />
            {/* 50¢ midline reference */}
            <ReferenceLine
              y={50}
              stroke="rgba(59, 130, 246, 0.15)"
              strokeDasharray="6 4"
            />
            {/* Yes price — primary line */}
            <Area
              type="monotone"
              dataKey="yes"
              stroke="#00ff88"
              strokeWidth={2.5}
              fill="url(#yesFill)"
              dot={<TradeDot />}
              activeDot={{
                r: 5,
                stroke: "#00ff88",
                strokeWidth: 2,
                fill: "var(--bg-deep)",
              }}
              isAnimationActive={false}
            />
            {/* No price — subtle complement */}
            <Area
              type="monotone"
              dataKey="no"
              stroke="#ff3366"
              strokeWidth={1}
              fill="none"
              dot={false}
              activeDot={{
                r: 3,
                stroke: "#ff3366",
                strokeWidth: 1,
                fill: "var(--bg-deep)",
              }}
              strokeDasharray="4 3"
              strokeOpacity={0.4}
              isAnimationActive={false}
            />
          </AreaChart>
        </ResponsiveContainer>
      )}

      {/* Legend */}
      <div className="flex items-center justify-center gap-6 mt-3">
        <span className="flex items-center gap-1.5 text-xs">
          <span className="w-4 h-0.5 bg-[var(--neon-green)] rounded" />
          <span className="text-[var(--text-secondary)]">
            Yes (Polymarket)
          </span>
        </span>
        <span className="flex items-center gap-1.5 text-xs">
          <span className="w-4 h-0.5 bg-[var(--neon-red)] rounded opacity-40" />
          <span className="text-[var(--text-secondary)]">No</span>
        </span>
        <span className="flex items-center gap-1.5 text-xs">
          <span className="w-3 h-3 rounded-full bg-[var(--accent-blue)] border-2 border-white/80" />
          <span className="text-[var(--text-secondary)]">
            On-chain Trades
          </span>
        </span>
      </div>
    </motion.div>
  );
}
