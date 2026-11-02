import { ChartNoAxesCombined } from "lucide-react";
import { useEffect, useId, useRef, useState } from "react";
import type { TrendPoint } from "../types";
import { formatTokens } from "../lib/format";

/** Source counts, never a deduplicated inference total. No smoothing or estimated samples. */
export function UsageChart({ points, label = "Telemetry token trend" }: { points: TrendPoint[]; label?: string }) {
  const gradientId = useId().replaceAll(":", "");
  const [active, setActive] = useState<number | null>(null);
  const [focusIndex, setFocusIndex] = useState(0);
  const [width, setWidth] = useState(760);
  const canvas = useRef<HTMLDivElement | null>(null);
  const buttons = useRef<(HTMLButtonElement | null)[]>([]);
  const empty = points.length === 0;
  useEffect(() => {
    if (!canvas.current || typeof ResizeObserver === "undefined") return;
    const observer = new ResizeObserver(([entry]) => { if (entry) setWidth(Math.max(1, entry.contentRect.width)); });
    observer.observe(canvas.current);
    return () => observer.disconnect();
  }, [empty]);
  if (points.length === 0) return <div className="chart-empty"><ChartNoAxesCombined /><strong>No measured trend yet</strong><span>Usage will appear here after the next measured event.</span></div>;
  const height = 210, left = 44, right = 12, top = 18, bottom = 33;
  const peak = Math.max(...points.map((point) => point.tokens), 0);
  const max = Math.max(peak, 1);
  const labelStep = Math.max(1, Math.ceil(points.length / Math.max(2, Math.floor(width / 95))));
  const chartWidth = width - left - right, chartHeight = height - top - bottom;
  const coords = points.map((point, index) => ({
    x: left + (points.length === 1 ? .5 : index / (points.length - 1)) * chartWidth,
    y: top + chartHeight * (1 - point.tokens / max),
  }));
  const line = coords.map((p, index) => `${index ? "L" : "M"}${p.x},${p.y}`).join(" ");
  const area = `${line} L${coords.at(-1)!.x},${height - bottom} L${coords[0]!.x},${height - bottom} Z`;
  const selected = active !== null ? points[active] : undefined;
  return <figure className="usage-chart" aria-label={label}>
    <div className="chart-readout" aria-live="polite">{selected ? <><span>{selected.label}</span><strong>{selected.tokens.toLocaleString()} <small>tokens</small></strong></> : <><span>Source tokens · may overlap</span><strong>Peak {formatTokens(peak)}</strong></>}</div>
    <div className="chart-canvas" ref={canvas} onMouseLeave={() => setActive(null)}>
      <svg viewBox={`0 0 ${width} ${height}`} aria-hidden="true">
        <defs><linearGradient id={gradientId} x1="0" y1="0" x2="0" y2="1"><stop offset="0%" stopColor="var(--accent)" stopOpacity=".16" /><stop offset="100%" stopColor="var(--accent)" stopOpacity=".015" /></linearGradient></defs>
        {[0, .5, 1].map((ratio) => <g key={ratio}><line className="chart-grid-line" x1={left} x2={width - right} y1={top + chartHeight * ratio} y2={top + chartHeight * ratio} /><text className="chart-label" x={left - 10} y={top + chartHeight * ratio + 4} textAnchor="end">{formatTokens(Math.round(max * (1 - ratio)))}</text></g>)}
        <path d={area} fill={`url(#${gradientId})`} />
        <path className="chart-line" d={line} />
        {points.length === 1 ? <circle className="chart-point" cx={coords[0]!.x} cy={coords[0]!.y} r="4" /> : null}
        {active !== null && coords[active] ? <g><line className="chart-cursor" x1={coords[active].x} x2={coords[active].x} y1={top} y2={height - bottom} /><circle className="chart-point" cx={coords[active].x} cy={coords[active].y} r="4" /></g> : null}
        {points.map((point, index) => (index % labelStep === 0 && index < points.length - 1 - labelStep / 2) || index === points.length - 1
          ? <text key={point.date} className="chart-label" x={coords[index]!.x} y={height - 8} textAnchor={index === 0 ? "start" : index === points.length - 1 ? "end" : "middle"}>{point.label}</text> : null)}
      </svg>
      <div className="chart-targets" style={{ left: `${left / width * 100}%`, right: `${right / width * 100}%` }}>
        {points.map((point, index) => <button key={point.date} type="button" ref={(element) => { buttons.current[index] = element; }}
          aria-label={`${point.label}: ${point.tokens.toLocaleString()} tokens`} tabIndex={index === Math.min(focusIndex, points.length - 1) ? 0 : -1}
          onMouseEnter={() => setActive(index)} onFocus={() => { setActive(index); setFocusIndex(index); }} onBlur={() => setActive(null)}
          onKeyDown={(event) => {
            let next: number;
            if (event.key === "ArrowRight") next = Math.min(points.length - 1, index + 1);
            else if (event.key === "ArrowLeft") next = Math.max(0, index - 1);
            else if (event.key === "Home") next = 0;
            else if (event.key === "End") next = points.length - 1;
            else return;
            event.preventDefault(); buttons.current[next]?.focus();
          }} />)}
      </div>
      {selected && active !== null && coords[active] ? <div className="chart-tooltip" aria-hidden="true" style={{ left: `${Math.min(82,Math.max(18,coords[active].x / width * 100))}%`, top: `${coords[active].y / height * 100}%` }}><span>{selected.label}</span><strong>{selected.tokens.toLocaleString()} tokens</strong></div> : null}
    </div>
    <figcaption>Exact values on hover or focus <span>← → to explore</span></figcaption>
  </figure>;
}
