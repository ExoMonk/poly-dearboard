import {
  AreaChart,
  Area,
  XAxis,
  YAxis,
  Tooltip,
  ReferenceLine,
  ResponsiveContainer,
  CartesianGrid,
} from "recharts";
import type { PnlChartPoint } from "../types";
import { formatUsd } from "../lib/format";

interface Props {
  points: PnlChartPoint[];
}

const TOOLTIP_STYLE = {
  backgroundColor: "rgba(12, 12, 30, 0.95)",
  border: "1px solid rgba(6, 182, 212, 0.2)",
  borderRadius: 8,
  fontSize: 13,
  boxShadow: "0 4px 20px rgba(0, 0, 0, 0.5)",
};

export default function PnlChart({ points }: Props) {
  const data = points.map((p) => ({
    date: new Date(p.date).toLocaleDateString("en-US", {
      month: "short",
      day: "numeric",
    }),
    pnl: parseFloat(p.pnl),
  }));

  if (data.length === 0) return null;

  const finalPnl = data[data.length - 1].pnl;
  const isPositive = finalPnl >= 0;
  const strokeColor = isPositive ? "var(--neon-green)" : "var(--neon-red)";
  const gradientId = isPositive ? "pnlGradGreen" : "pnlGradRed";
  const gradientColor = isPositive ? "#00ff88" : "#ff3366";

  return (
    <div className="glass p-5 gradient-border-top shimmer-border">
      <h3 className="text-sm font-medium text-[var(--text-secondary)] mb-4 uppercase tracking-wider">
        Cumulative P&L
      </h3>
      <ResponsiveContainer width="100%" height={250}>
        <AreaChart
          data={data}
          margin={{ left: 10, right: 10, top: 10, bottom: 0 }}
        >
          <defs>
            <linearGradient id={gradientId} x1="0" y1="0" x2="0" y2="1">
              <stop offset="0%" stopColor={gradientColor} stopOpacity={0.25} />
              <stop
                offset="100%"
                stopColor={gradientColor}
                stopOpacity={0.02}
              />
            </linearGradient>
          </defs>
          <CartesianGrid
            strokeDasharray="3 3"
            stroke="rgba(6, 182, 212, 0.06)"
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
          <Tooltip
            contentStyle={TOOLTIP_STYLE}
            labelStyle={{ color: "var(--accent-cyan)" }}
            formatter={(value: number | undefined) => [formatUsd(String(value ?? 0)), "P&L"]}
          />
          <ReferenceLine y={0} stroke="rgba(100, 116, 139, 0.3)" />
          <Area
            type="monotone"
            dataKey="pnl"
            stroke={strokeColor}
            strokeWidth={2}
            fill={`url(#${gradientId})`}
            animationDuration={800}
          />
        </AreaChart>
      </ResponsiveContainer>
    </div>
  );
}
