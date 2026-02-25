import { useState, useEffect, useRef } from "react";
import {
  createChart,
  AreaSeries,
  LineSeries,
  createSeriesMarkers,
  CrosshairMode,
  ColorType,
  LineType,
  type IChartApi,
  type ISeriesApi,
  type ISeriesMarkersPluginApi,
  type UTCTimestamp,
  type Time,
} from "lightweight-charts";
import { motion } from "motion/react";
import type { PricePoint, MarketWsStatus, BidAsk } from "../types";
import { panelVariants } from "../lib/motion";

interface Props {
  priceHistory: PricePoint[];
  tradeMarkers: Set<number>;
  status: MarketWsStatus;
  bidAsk: BidAsk;
}

const WINDOWS = { "5m": 300, "15m": 900 } as const;
type TimeWindow = keyof typeof WINDOWS;

function toSec(ms: number): UTCTimestamp {
  return Math.floor(ms / 1000) as UTCTimestamp;
}

/** Deduplicate price points into {time (sec), value (cents)}, optionally filtered by timestamp set. */
function buildSeries(
  points: PricePoint[],
  filter?: Set<number>,
): { time: UTCTimestamp; value: number }[] {
  const map = new Map<number, number>();
  for (const p of points) {
    if (filter && !filter.has(p.timestamp)) continue;
    const sec = Math.floor(p.timestamp / 1000);
    if (!Number.isFinite(sec) || !Number.isFinite(p.yesPrice)) continue;
    map.set(sec, p.yesPrice * 100);
  }
  return Array.from(map.entries())
    .sort((a, b) => a[0] - b[0])
    .map(([t, v]) => ({ time: t as UTCTimestamp, value: v }));
}

export default function LivePriceChart({
  priceHistory,
  tradeMarkers,
  status,
  bidAsk,
}: Props) {
  const containerRef = useRef<HTMLDivElement>(null);
  const chartRef = useRef<IChartApi | null>(null);
  const polyRef = useRef<ISeriesApi<"Area"> | null>(null);
  const noRef = useRef<ISeriesApi<"Line"> | null>(null);
  const chainRef = useRef<ISeriesApi<"Line"> | null>(null);
  const markersRef = useRef<ISeriesMarkersPluginApi<Time> | null>(null);
  const tooltipRef = useRef<HTMLDivElement | null>(null);
  const dotRef = useRef<HTMLDivElement | null>(null);
  const latestRef = useRef<number | null>(null);
  const animRef = useRef<number | null>(null); // current lerp'd value
  const rangeSetRef = useRef(false); // whether initial range has been set
  const prevHistLenRef = useRef(0); // track priceHistory length to detect changes
  const windowRef = useRef<TimeWindow>("5m");
  const [timeWindow, setTimeWindow] = useState<TimeWindow>("5m");

  windowRef.current = timeWindow;

  useEffect(() => {
    if (priceHistory.length > 0) {
      latestRef.current =
        priceHistory[priceHistory.length - 1].yesPrice * 100;
    }
  }, [priceHistory]);

  // ── Create chart (mount only) ─────────────────────────────────────
  useEffect(() => {
    const el = containerRef.current;
    if (!el) return;

    const chart = createChart(el, {
      width: el.clientWidth,
      height: 280,
      layout: {
        background: { type: ColorType.Solid, color: "transparent" },
        textColor: "rgba(148, 163, 184, 0.8)",
        fontSize: 10,
      },
      grid: {
        vertLines: { visible: false },
        horzLines: { color: "rgba(59, 130, 246, 0.06)" },
      },
      crosshair: {
        mode: CrosshairMode.Normal,
        vertLine: {
          color: "rgba(59, 130, 246, 0.3)",
          labelBackgroundColor: "rgba(10, 18, 40, 0.9)",
        },
        horzLine: {
          color: "rgba(59, 130, 246, 0.3)",
          labelBackgroundColor: "rgba(10, 18, 40, 0.9)",
        },
      },
      timeScale: {
        timeVisible: true,
        secondsVisible: true,
        borderVisible: false,
        rightOffset: 5,
        shiftVisibleRangeOnNewBar: true,
      },
      rightPriceScale: {
        borderVisible: false,
        scaleMargins: { top: 0, bottom: 0 },
      },
      handleScroll: { mouseWheel: true, pressedMouseMove: true },
      handleScale: { mouseWheel: true, pinch: true },
    });

    // Series 1 — Polymarket price (green area, curved spline)
    const poly = chart.addSeries(AreaSeries, {
      topColor: "rgba(0, 255, 136, 0.15)",
      bottomColor: "rgba(0, 255, 136, 0.02)",
      lineColor: "#00ff88",
      lineWidth: 2,
      lineType: LineType.Curved,
      priceFormat: {
        type: "custom",
        formatter: (p: number) => `${p.toFixed(1)}\u00a2`,
      },
      crosshairMarkerVisible: true,
      crosshairMarkerRadius: 5,
      crosshairMarkerBorderColor: "#00ff88",
      crosshairMarkerBackgroundColor: "rgba(10, 18, 40, 0.9)",
      // Fix Y axis to 0¢–100¢
      autoscaleInfoProvider: () => ({
        priceRange: { minValue: 0, maxValue: 100 },
      }),
    });

    // Series 2 — No/complement price (red line, no fill)
    const no = chart.addSeries(LineSeries, {
      color: "rgba(255, 51, 102, 0.5)",
      lineWidth: 1,
      lineType: LineType.Curved,
      priceFormat: {
        type: "custom",
        formatter: (p: number) => `${p.toFixed(1)}\u00a2`,
      },
      crosshairMarkerVisible: false,
      autoscaleInfoProvider: () => ({
        priceRange: { minValue: 0, maxValue: 100 },
      }),
    });

    // Series 3 — On-chain indexed trades (small markers only, no line)
    const chain = chart.addSeries(LineSeries, {
      color: "#3b82f6",
      lineWidth: 1,
      lineVisible: false,
      pointMarkersVisible: false,
      priceFormat: {
        type: "custom",
        formatter: (p: number) => `${p.toFixed(1)}\u00a2`,
      },
      crosshairMarkerVisible: true,
      crosshairMarkerRadius: 3,
      crosshairMarkerBorderColor: "#3b82f6",
      crosshairMarkerBackgroundColor: "rgba(10, 18, 40, 0.9)",
    });

    const markers = createSeriesMarkers(chain, []);

    chartRef.current = chart;
    polyRef.current = poly;
    noRef.current = no;
    chainRef.current = chain;
    markersRef.current = markers;

    // ── Custom tooltip ──────────────────────────────────────────────
    const tip = document.createElement("div");
    tip.style.cssText = [
      "position:absolute",
      "display:none",
      "z-index:10",
      "pointer-events:none",
      "background:rgba(10,18,40,0.95)",
      "border:1px solid rgba(59,130,246,0.2)",
      "border-radius:8px",
      "padding:10px 14px",
      "font-size:12px",
      "box-shadow:0 4px 20px rgba(0,0,0,0.5)",
    ].join(";");
    el.style.position = "relative";
    el.appendChild(tip);
    tooltipRef.current = tip;

    const tsEl = document.createElement("div");
    tsEl.style.cssText = "color:var(--accent-blue);margin-bottom:6px;font-size:11px;font-family:monospace";

    const gridEl = document.createElement("div");
    gridEl.style.cssText = "display:grid;grid-template-columns:auto auto;gap:2px 12px";

    const yesLabelEl = document.createElement("span");
    yesLabelEl.style.cssText = "color:#00ff88";
    yesLabelEl.textContent = "Yes";

    const yesValueEl = document.createElement("span");
    yesValueEl.style.cssText = "color:#00ff88;font-family:monospace";

    const noLabelEl = document.createElement("span");
    noLabelEl.style.cssText = "color:#ff3366";
    noLabelEl.textContent = "No";

    const noValueEl = document.createElement("span");
    noValueEl.style.cssText = "color:#ff3366;font-family:monospace";

    const chainTagEl = document.createElement("div");
    chainTagEl.style.cssText = "color:#3b82f6;font-size:10px;margin-top:4px;display:none";
    chainTagEl.textContent = "On-chain trade";

    gridEl.append(yesLabelEl, yesValueEl, noLabelEl, noValueEl);
    tip.append(tsEl, gridEl, chainTagEl);

    chart.subscribeCrosshairMove((param) => {
      if (!param.point || param.time === undefined) {
        tip.style.display = "none";
        return;
      }
      const pVal = (
        param.seriesData.get(poly) as { value?: number } | undefined
      )?.value;
      const cVal = (
        param.seriesData.get(chain) as { value?: number } | undefined
      )?.value;
      const yes = pVal ?? cVal;
      if (yes == null) {
        tip.style.display = "none";
        return;
      }

      const no = 100 - yes;
      const d = new Date((param.time as number) * 1000);
      const ts = d.toLocaleTimeString("en-US", {
        hour: "2-digit",
        minute: "2-digit",
        second: "2-digit",
        hour12: false,
      });

      tip.style.display = "block";
      tsEl.textContent = ts;
      yesValueEl.textContent = `${yes.toFixed(1)}\u00a2`;
      noValueEl.textContent = `${no.toFixed(1)}\u00a2`;
      chainTagEl.style.display = cVal != null ? "block" : "none";

      const left =
        param.point.x > el.clientWidth * 0.6
          ? param.point.x - tip.offsetWidth - 15
          : param.point.x + 15;
      tip.style.left = `${Math.max(0, left)}px`;
      tip.style.top = `${Math.max(0, param.point.y - 30)}px`;
    });

    // ── Glowing leading-edge dot ────────────────────────────────────
    const dot = document.createElement("div");
    dot.style.cssText = [
      "position:absolute",
      "width:8px",
      "height:8px",
      "border-radius:50%",
      "background:#00ff88",
      "box-shadow:0 0 6px #00ff88,0 0 14px rgba(0,255,136,0.4)",
      "pointer-events:none",
      "z-index:5",
      "display:none",
      "transform:translate(-50%,-50%)",
    ].join(";");
    el.appendChild(dot);
    dotRef.current = dot;

    // ── Resize ──────────────────────────────────────────────────────
    const ro = new ResizeObserver((entries) => {
      chart.applyOptions({ width: entries[0].contentRect.width });
    });
    ro.observe(el);

    // ── 60fps lerp loop: smooth interpolation toward latest price ──
    const LERP_SPEED = 0.08; // 8% per frame — same as Liveline
    let frameId = 0;

    const animate = () => {
      const target = latestRef.current;
      if (target != null) {
        // Initialize animRef on first value
        if (animRef.current == null) animRef.current = target;

        // Lerp toward target
        const diff = target - animRef.current;
        if (Math.abs(diff) > 0.01) {
          animRef.current += diff * LERP_SPEED;
        } else {
          animRef.current = target;
        }

        const now = toSec(Date.now());
        poly.update({ time: now, value: animRef.current });
        no.update({ time: now, value: 100 - animRef.current });

        // Position glowing dot at leading edge
        const x = chart.timeScale().timeToCoordinate(now);
        const y = poly.priceToCoordinate(animRef.current);
        if (x !== null && y !== null) {
          dot.style.display = "block";
          dot.style.left = `${x}px`;
          dot.style.top = `${y}px`;
        } else {
          dot.style.display = "none";
        }
      }

      frameId = requestAnimationFrame(animate);
    };
    frameId = requestAnimationFrame(animate);

    return () => {
      cancelAnimationFrame(frameId);
      ro.disconnect();
      dot.remove();
      tip.remove();
      chart.remove();
    };
  }, []); // eslint-disable-line react-hooks/exhaustive-deps

  // ── Update data on priceHistory / tradeMarkers change ─────────────
  // Always rebuild poly/no series from priceHistory so new trades AND WSS
  // ticks are reflected. The immediate update() to "now" with the lerp value
  // runs synchronously (same JS task), so the browser never paints the
  // intermediate state — no visual flash.
  useEffect(() => {
    if (!polyRef.current || !chainRef.current || !markersRef.current) return;

    if (priceHistory.length > prevHistLenRef.current && priceHistory.length > 0) {
      const allData = buildSeries(priceHistory);
      if (allData.length > 0) {
        polyRef.current.setData(allData);
        noRef.current?.setData(
          allData.map((d) => ({ time: d.time, value: 100 - d.value })),
        );
        // Extend to "now" with current lerped value to prevent visual snap
        if (animRef.current != null) {
          const now = toSec(Date.now());
          polyRef.current.update({ time: now, value: animRef.current });
          noRef.current?.update({ time: now, value: 100 - animRef.current });
        }
      }
    }
    prevHistLenRef.current = priceHistory.length;

    // Chain series (on-chain trade dots) — setData is fine, no visible line
    const chainData = buildSeries(priceHistory, tradeMarkers);
    chainRef.current.setData(chainData);

    markersRef.current.setMarkers(
      chainData.map((d) => ({
        time: d.time,
        position: "inBar" as const,
        shape: "circle" as const,
        color: "rgba(59, 130, 246, 0.6)",
        size: 0.5,
      })),
    );

    // Set visible range only once on initial data load
    if (!rangeSetRef.current && chartRef.current && priceHistory.length >= 2) {
      rangeSetRef.current = true;
      const now = toSec(Date.now());
      try {
        const sec = WINDOWS[windowRef.current];
        chartRef.current.timeScale().setVisibleRange({
          from: (now - sec) as UTCTimestamp,
          to: (now + 5) as UTCTimestamp,
        });
      } catch {
        // Chart may not be ready
      }
    }
  }, [priceHistory, tradeMarkers]);

  // ── Snap visible range on time-window change ──────────────────────
  useEffect(() => {
    if (!chartRef.current || priceHistory.length < 2) return;
    const now = toSec(Date.now());
    try {
      chartRef.current.timeScale().setVisibleRange({
        from: (now - WINDOWS[timeWindow]) as UTCTimestamp,
        to: (now + 3) as UTCTimestamp,
      });
    } catch {
      // Chart may not have data yet
    }
  }, [timeWindow, priceHistory.length]);

  const currentYes =
    priceHistory.length > 0
      ? priceHistory[priceHistory.length - 1].yesPrice * 100
      : null;

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
          {currentYes != null && (
            <span className="text-lg font-bold font-mono text-[var(--neon-green)]">
              {currentYes.toFixed(1)}&cent;
            </span>
          )}
        </div>
        <div className="flex items-center gap-3">
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
      <div className="relative" style={{ height: 280 }}>
        <div
          ref={containerRef}
          className="absolute inset-0"
          style={{
            opacity: priceHistory.length < 2 ? 0 : 1,
            transition: "opacity 0.3s",
          }}
        />
        {priceHistory.length < 2 && (
          <div className="absolute inset-0 flex items-center justify-center">
            <p className="text-[var(--text-secondary)] text-sm">
              Waiting for price data...
            </p>
          </div>
        )}
      </div>

      {/* Legend */}
      <div className="flex items-center justify-center gap-6 mt-3">
        <span className="flex items-center gap-1.5 text-xs">
          <span className="w-4 h-0.5 bg-[var(--neon-green)] rounded" />
          <span className="text-[var(--text-secondary)]">Yes</span>
        </span>
        <span className="flex items-center gap-1.5 text-xs">
          <span className="w-4 h-0.5 bg-[#ff3366] opacity-50 rounded" />
          <span className="text-[var(--text-secondary)]">No</span>
        </span>
        <span className="flex items-center gap-1.5 text-xs">
          <span className="w-4 h-0.5 bg-[#3b82f6] rounded" />
          <span className="text-[var(--text-secondary)]">
            On-chain Trades
          </span>
        </span>
      </div>
    </motion.div>
  );
}
