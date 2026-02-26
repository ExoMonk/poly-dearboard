import { memo } from "react";
import {
  BarChart,
  Bar,
  XAxis,
  YAxis,
  Tooltip,
  ReferenceLine,
  ResponsiveContainer,
  CartesianGrid,
  Cell,
} from "recharts";
import { motion } from "motion/react";
import type { PnlBar, PnlTimeframe } from "../types";
import { formatUsd } from "../lib/format";
import { panelVariants, tapScale } from "../lib/motion";

interface Props {
  bars: PnlBar[];
  timeframe: PnlTimeframe;
  onTimeframeChange: (tf: PnlTimeframe) => void;
}

const TIMEFRAMES: { value: PnlTimeframe; label: string }[] = [
  { value: "24h", label: "24H" },
  { value: "7d", label: "7D" },
  { value: "30d", label: "30D" },
  { value: "all", label: "All" },
];

const TOOLTIP_STYLE = {
  backgroundColor: "rgba(10, 18, 40, 0.95)",
  border: "1px solid rgba(59, 130, 246, 0.2)",
  borderRadius: 8,
  fontSize: 13,
  boxShadow: "0 4px 20px rgba(0, 0, 0, 0.5)",
};

const GREEN = "#00ff88";
const RED = "#ff3366";

function formatDateLabel(dateStr: string, timeframe: PnlTimeframe): string {
  const isHourly = dateStr.includes(" ");
  const d = new Date(isHourly ? dateStr.replace(" ", "T") : dateStr);
  if (isNaN(d.getTime())) return dateStr;

  if (isHourly || timeframe === "24h") {
    return d.toLocaleTimeString("en-US", { hour: "2-digit", minute: "2-digit", hour12: false });
  }
  return d.toLocaleDateString("en-US", { month: "short", day: "numeric" });
}

// eslint-disable-next-line @typescript-eslint/no-explicit-any
function CustomTooltip({ active, payload, label }: any) {
  if (!active || !payload?.length) return null;
  const realized = payload.find((p: any) => p.dataKey === "realized")?.value as number | undefined;
  const unrealized = payload.find((p: any) => p.dataKey === "unrealized")?.value as number | undefined;
  return (
    <div style={TOOLTIP_STYLE} className="px-3 py-2">
      <p style={{ color: "var(--accent-blue)", marginBottom: 4, fontSize: 12 }}>{label}</p>
      {realized !== undefined && realized !== 0 && (
        <p style={{ color: realized >= 0 ? GREEN : RED, fontSize: 13 }}>
          Realized: {formatUsd(String(realized))}
        </p>
      )}
      {unrealized !== undefined && unrealized !== 0 && (
        <p style={{ color: unrealized >= 0 ? GREEN : RED, fontSize: 13, opacity: 0.7 }}>
          Unrealized: {formatUsd(String(unrealized))}
        </p>
      )}
      {(realized === 0 || realized === undefined) && (unrealized === 0 || unrealized === undefined) && (
        <p style={{ color: "var(--text-secondary)", fontSize: 13 }}>No activity</p>
      )}
    </div>
  );
}

export default memo(function PnlChart({ bars, timeframe, onTimeframeChange }: Props) {
  const data = bars.map((b) => ({
    date: formatDateLabel(b.date, timeframe),
    realized: parseFloat(b.realized),
    unrealized: b.unrealized != null ? parseFloat(b.unrealized) : 0,
    hasUnrealized: b.unrealized != null,
  }));

  return (
    <motion.div
      variants={panelVariants}
      initial="initial"
      animate="animate"
      transition={{ duration: 0.4 }}
      className="glass p-5 gradient-border-top"
    >
      <div className="flex items-center justify-between mb-4">
        <h3 className="text-sm font-medium text-[var(--text-secondary)] uppercase tracking-wider">
          P&L
        </h3>
        <div className="flex gap-1">
          {TIMEFRAMES.map((tf) => (
            <motion.button
              key={tf.value}
              onClick={() => onTimeframeChange(tf.value)}
              whileTap={tapScale}
              className={`px-3 py-1 text-xs rounded-full font-medium transition-all duration-200 ${
                timeframe === tf.value
                  ? "bg-[var(--accent-blue)]/10 text-[var(--accent-blue)] border border-[var(--accent-blue)]/30 shadow-[0_0_8px_rgba(59,130,246,0.15)]"
                  : "text-[var(--text-secondary)] border border-transparent hover:text-[var(--text-primary)] hover:border-[var(--border-glow)]"
              }`}
            >
              {tf.label}
            </motion.button>
          ))}
        </div>
      </div>
      {data.length === 0 ? (
        <p className="text-[var(--text-secondary)] text-center py-16 text-sm">No trades in this period</p>
      ) : (
        <ResponsiveContainer width="100%" height={250}>
          <BarChart
            data={data}
            margin={{ left: 10, right: 10, top: 10, bottom: 0 }}
            stackOffset="sign"
          >
            <CartesianGrid
              strokeDasharray="3 3"
              stroke="rgba(59, 130, 246, 0.06)"
              vertical={false}
            />
            <XAxis
              dataKey="date"
              tick={{ fill: "var(--text-secondary)", fontSize: 11 }}
              axisLine={false}
              tickLine={false}
            />
            <YAxis
              tick={{ fill: "var(--text-secondary)", fontSize: 11 }}
              tickFormatter={(v: number) => formatUsd(String(v))}
              axisLine={false}
              tickLine={false}
            />
            <Tooltip content={<CustomTooltip />} cursor={{ fill: "rgba(59, 130, 246, 0.05)" }} />
            <ReferenceLine y={0} stroke="rgba(100, 116, 139, 0.3)" />
            <Bar dataKey="realized" stackId="pnl" animationDuration={800} radius={[2, 2, 0, 0]}>
              {data.map((d, i) => (
                <Cell key={i} fill={d.realized >= 0 ? GREEN : RED} fillOpacity={0.85} />
              ))}
            </Bar>
            <Bar dataKey="unrealized" stackId="pnl" animationDuration={800} radius={[2, 2, 0, 0]}>
              {data.map((d, i) => (
                <Cell
                  key={i}
                  fill={d.unrealized >= 0 ? GREEN : RED}
                  fillOpacity={d.hasUnrealized ? 0.35 : 0}
                  strokeDasharray={d.hasUnrealized ? "4 2" : undefined}
                  stroke={d.hasUnrealized ? (d.unrealized >= 0 ? GREEN : RED) : "none"}
                />
              ))}
            </Bar>
          </BarChart>
        </ResponsiveContainer>
      )}
    </motion.div>
  );
});
