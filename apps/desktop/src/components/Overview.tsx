import { useState } from "react";
import { ArrowUpRight, Calculator, CircleDollarSign, DatabaseZap, RefreshCw } from "lucide-react";
import type { BreakdownItem, DashboardSnapshot, ProviderQuotaState, SourceScanResult, TrendPoint } from "../types";
import { formatMinorCurrency, formatMinutes, formatQuotaPercent, formatQuotaReset, formatRelativeTime, formatTokens, formatUsdCents, formatUsdMicros } from "../lib/format";
import { ProviderMark } from "./ProviderMark";

interface OverviewProps {
  data: DashboardSnapshot;
  claudeQuota?: ProviderQuotaState | null;
  scanning: boolean;
  onScan: () => void;
}
export function Overview({ data, claudeQuota, scanning, onScan }: OverviewProps) {
  const [scenarioMultiplier, setScenarioMultiplier] = useState(2);
  if (data.metrics.measuredEventsRange === 0 && data.metrics.measuredTokensMonth === 0 && data.metrics.activityMinutesRange === 0) {
    return <div className="page-stack overview-page"><LiveLimits quota={claudeQuota} /><Onboarding sources={data.sources} scanning={scanning} onScan={onScan} /></div>;
  }

  const metrics = data.metrics;
  const hasPricedValue = metrics.estimatedApiValueUsdMicros !== null;
  const pricedValueMicros = metrics.estimatedApiValueUsdMicros ?? 0;
  const pricingCoverage = metrics.measuredTokensRange > 0
    ? metrics.pricedTokensRange / metrics.measuredTokensRange
    : metrics.measuredEventsRange > 0
      ? metrics.pricedEventsRange / metrics.measuredEventsRange
      : 0;
  const pricingCoveragePercent = Math.min(100, Math.max(0, Math.round(pricingCoverage * 100)));
  const valueLabel = hasPricedValue
    ? `${formatUsdMicros(pricedValueMicros)}${metrics.pricingComplete ? "" : "+"}`
    : "Unavailable";
  const valueContext = metrics.pricingComplete
    ? "Based on versioned model pricing"
    : hasPricedValue
      ? `Partial estimate · ${pricingCoveragePercent}% of measured tokens priced`
      : "No measured events have safe model pricing";
  return (
    <div className="page-stack overview-page">
      <LiveLimits quota={claudeQuota} />
      <section className="hero-metrics" aria-label="Headline usage metrics">
        <div className="hero-primary">
          <p className="eyebrow">Measured tokens · selected period</p>
          <div className="metric-number">{formatTokens(metrics.measuredTokensRange)}</div>
          <p className="metric-footnote">{metrics.measuredEventsRange.toLocaleString()} authoritative local usage events</p>
        </div>
        <div className="metric-divider" />
        <Metric label="Today" value={formatTokens(metrics.measuredTokensToday)} suffix="measured" />
        <Metric label="This month" value={formatTokens(metrics.measuredTokensMonth)} suffix="measured" />
        <Metric label="Active time" value={formatMinutes(metrics.activityMinutesRange)} suffix="activity-only" />
        <Metric label="Subscriptions" value={formatUsdCents(metrics.monthlySubscriptionUsdCents)} suffix="monthly" />
      </section>

      <section className="value-strip">
        <div className="value-icon"><CircleDollarSign /></div>
        <div>
          <span className="eyebrow">Historical public API list value</span>
          <strong>{valueLabel}</strong>
        </div>
        <div className="value-context">
          {valueContext}
        </div>
        <div className="value-multiple">
          <span>Pricing coverage</span>
          <strong>{pricingCoveragePercent}%</strong>
        </div>
      </section>

      <section className="panel value-calculator" aria-labelledby="value-calculator-title">
        <div className="calculator-heading">
          <div className="calculator-icon"><Calculator /></div>
          <div>
            <h2 id="value-calculator-title">Unsubsidized price scenario</h2>
            <p>Explore a hypothetical markup over the verified public API list value.</p>
          </div>
        </div>
        <div className="calculator-result">
          <span>Scenario value</span>
          <strong>{hasPricedValue ? `${formatUsdMicros(Math.round(pricedValueMicros * scenarioMultiplier))}${metrics.pricingComplete ? "" : "+"}` : "Unavailable"}</strong>
          <small>{hasPricedValue ? `${formatUsdMicros(pricedValueMicros)} × ${scenarioMultiplier.toFixed(1)}` : "No priceable usage in this period"}</small>
        </div>
        <label className="calculator-control">
          <span>Scenario multiplier</span>
          <div>
            <input
              aria-label="Scenario multiplier"
              type="range"
              min="1"
              max="10"
              step="0.5"
              value={scenarioMultiplier}
              onChange={(event) => setScenarioMultiplier(Number(event.target.value))}
            />
            <output>{scenarioMultiplier.toFixed(1)}×</output>
          </div>
        </label>
        <p className="calculator-note">Public API list price is the closest verifiable MSRP. Providers do not publish a definitive “unsubsidized” consumer price, so values above 1× are scenarios—not claims about provider costs.</p>
      </section>

      <div className="overview-grid">
        <section className="panel trend-panel">
          <PanelHeader title="Usage trend" subtitle="Measured tokens only" />
          <UsageChart points={data.trend} />
        </section>
        <section className="panel provider-panel">
          <PanelHeader title="By provider" subtitle="Share of selected period" />
          <BreakdownList items={data.byProvider} providerMarks />
        </section>
      </div>

      <div className="overview-grid overview-grid-bottom">
        <section className="panel">
          <PanelHeader title="Top models" subtitle="Measured usage" />
          <BreakdownList items={data.byModel.slice(0, 5)} />
        </section>
        <section className="panel">
          <PanelHeader title="Projects" subtitle="Sanitized local basenames" />
          <BreakdownList items={data.byProject.slice(0, 5)} />
        </section>
        <section className="panel">
          <PanelHeader title="Devices" subtitle="Combined local ledger" />
          <BreakdownList items={data.byDevice.slice(0, 5)} />
        </section>
      </div>
    </div>
  );
}

function LiveLimits({ quota }: { quota?: ProviderQuotaState | null }) {
  if (!quota?.enabled) return null;
  const source = quota.sourceDeviceName ? ` from ${quota.sourceDeviceName}` : "";
  return (
    <section className={`panel live-limits ${quota.stale ? "live-limits-stale" : ""}`} aria-labelledby="live-limits-title">
      <div className="live-limits-heading">
        <div><p className="eyebrow">Claude account</p><h2 id="live-limits-title">Live limits</h2></div>
        <div className="live-limits-freshness">
          <span className={`status-dot status-${quota.status === "healthy" ? "healthy" : "warning"}`} />
          <span>{quota.observedAt ? `Updated ${formatRelativeTime(quota.observedAt)}${source}` : quota.message}</span>
        </div>
      </div>
      {quota.windows.length > 0 ? (
        <div className="quota-window-list">
          {quota.windows.map((window) => (
            <div className="quota-window" key={window.key}>
              <div><strong>{window.label}</strong><small>{formatQuotaReset(window.resetsAt)}</small></div>
              <div className="quota-track" aria-label={`${window.label} ${formatQuotaPercent(window.utilizationBps)} used`}><span style={{ width: `${window.utilizationBps / 100}%` }} /></div>
              <strong>{formatQuotaPercent(window.utilizationBps)} <small>used</small></strong>
            </div>
          ))}
        </div>
      ) : <p className="quota-empty">{quota.message}</p>}
      {quota.extraUsage ? (
        <div className="extra-usage-row">
          <div><strong>Extra usage</strong><small>{quota.extraUsage.enabled ? "Enabled" : "Disabled"}</small></div>
          {quota.extraUsage.enabled ? <><span>Used this month <strong>{formatMinorCurrency(quota.extraUsage.usedCreditsMinor, quota.extraUsage.currency)}</strong></span><span>Monthly limit <strong>{formatMinorCurrency(quota.extraUsage.monthlyLimitMinor, quota.extraUsage.currency)}</strong></span></> : null}
        </div>
      ) : null}
      {quota.stale ? <p className="quota-warning">{quota.message}</p> : null}
    </section>
  );
}

function Metric({ label, value, suffix }: { label: string; value: string; suffix: string }) {
  return (
    <div className="metric-secondary">
      <span>{label}</span>
      <strong>{value}</strong>
      <small>{suffix}</small>
    </div>
  );
}

function PanelHeader({ title, subtitle }: { title: string; subtitle: string }) {
  return (
    <div className="panel-heading">
      <div>
        <h2>{title}</h2>
        <p>{subtitle}</p>
      </div>
    </div>
  );
}

function UsageChart({ points }: { points: TrendPoint[] }) {
  if (points.length === 0) {
    return <div className="chart-empty">Usage will appear here after the next measured event.</div>;
  }
  const width = 760;
  const height = 250;
  const padX = 16;
  const padTop = 18;
  const padBottom = 35;
  const max = Math.max(...points.map((point) => point.tokens), 1);
  const usableWidth = width - padX * 2;
  const usableHeight = height - padTop - padBottom;
  const coords = points.map((point, index) => ({
    x: padX + (index / Math.max(points.length - 1, 1)) * usableWidth,
    y: padTop + usableHeight - (point.tokens / max) * usableHeight,
  }));
  const line = coords.map((point, index) => `${index === 0 ? "M" : "L"}${point.x.toFixed(1)},${point.y.toFixed(1)}`).join(" ");
  const area = `${line} L${coords.at(-1)?.x ?? padX},${height - padBottom} L${padX},${height - padBottom} Z`;
  const labels = points.length > 7 ? points.filter((_, index) => index % Math.ceil(points.length / 6) === 0 || index === points.length - 1) : points;

  return (
    <div className="chart-wrap">
      <svg viewBox={`0 0 ${width} ${height}`} role="img" aria-label="Measured token usage trend">
        <defs>
          <linearGradient id="usage-fill" x1="0" y1="0" x2="0" y2="1">
            <stop offset="0%" stopColor="var(--accent)" stopOpacity="0.28" />
            <stop offset="100%" stopColor="var(--accent)" stopOpacity="0" />
          </linearGradient>
        </defs>
        {[0.25, 0.5, 0.75, 1].map((lineValue) => (
          <line key={lineValue} className="chart-grid-line" x1={padX} x2={width - padX} y1={padTop + usableHeight * lineValue} y2={padTop + usableHeight * lineValue} />
        ))}
        <path className="chart-area" d={area} />
        <path className="chart-line" d={line} />
        {coords.map((point, index) => <circle key={points[index]?.date} className="chart-point" cx={point.x} cy={point.y} r="3" />)}
        {labels.map((point) => {
          const index = points.indexOf(point);
          const x = coords[index]?.x ?? padX;
          return <text key={point.date} className="chart-label" x={x} y={height - 8} textAnchor={index === 0 ? "start" : index === points.length - 1 ? "end" : "middle"}>{point.label}</text>;
        })}
      </svg>
      <div className="chart-peak">Peak <strong>{formatTokens(max)}</strong></div>
    </div>
  );
}

function BreakdownList({ items, providerMarks = false }: { items: BreakdownItem[]; providerMarks?: boolean }) {
  if (items.length === 0) return <div className="list-empty">No measured usage in this period.</div>;
  return (
    <div className="breakdown-list">
      {items.map((item) => (
        <div className="breakdown-row" key={item.key}>
          <div className="breakdown-label">
            {providerMarks ? <ProviderMark provider={item.key} size="small" /> : null}
            <span title={item.label}>{item.label}</span>
          </div>
          <div className="breakdown-track"><span style={{ width: `${Math.max(item.percentage, 1)}%` }} /></div>
          <strong>{formatTokens(item.tokens)}</strong>
          <small>{item.percentage.toFixed(0)}%</small>
        </div>
      ))}
    </div>
  );
}

function Onboarding({ sources, scanning, onScan }: { sources: SourceScanResult[]; scanning: boolean; onScan: () => void }) {
  const detected = sources.filter((source) => source.detected).length;
  return (
    <div className="onboarding">
      <div className="onboarding-copy">
        <div className="onboarding-icon"><DatabaseZap /></div>
        <p className="eyebrow">Private by default · local-first</p>
        <h1>Connect your AI usage</h1>
        <p>ArcMeter discovers trusted local telemetry automatically. Prompts, responses, source code, and full file paths never enter the usage ledger.</p>
        <button type="button" className="primary-button" onClick={onScan} disabled={scanning}>
          <RefreshCw className={scanning ? "spin" : ""} />
          {scanning ? "Scanning local sources" : detected > 0 ? "Scan detected sources" : "Scan this computer"}
        </button>
      </div>
      <div className="detection-list" aria-label="AI source detection">
        <div className="detection-heading"><span>AI sources</span><small>{detected} of {sources.length} detected</small></div>
        {sources.map((source) => (
          <div className="detection-row" key={source.provider}>
            <ProviderMark provider={source.provider} />
            <div>
              <strong>{source.label}</strong>
              <span>{source.detected ? source.measuredRecords > 0 ? `${source.measuredRecords.toLocaleString()} measured events` : "Detected · awaiting measured usage" : "Not detected"}</span>
            </div>
            <span className={`status-dot status-${source.detected ? source.status : "idle"}`} />
            <small>{source.lastUsageAt ? formatRelativeTime(source.lastUsageAt) : source.detected ? "Ready" : "—"}</small>
          </div>
        ))}
        <div className="privacy-note"><ArrowUpRight /> Only normalized token metadata is eligible for sync.</div>
      </div>
    </div>
  );
}
