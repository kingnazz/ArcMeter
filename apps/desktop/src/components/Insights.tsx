import { BarChart3, FolderKanban, TrendingUp } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import type { BreakdownItem, CacheEfficiencyBreakdown, CacheEfficiencyReport, CacheRange, Insight, ProviderQuotaState } from "../types";
import { getCacheEfficiency } from "../lib/api";
import { formatProjectedDuration, formatQuotaPace, formatQuotaPercent, formatQuotaReset, formatTokens, formatUsdMicrosPrecise, providerLabel } from "../lib/format";

interface InsightsProps {
  insights: Insight[];
  byModel: BreakdownItem[];
  byProject: BreakdownItem[];
  quotas?: (ProviderQuotaState | null)[];
  initialCache?: CacheEfficiencyReport;
  loadCache?: (range: CacheRange, provider?: string) => Promise<CacheEfficiencyReport>;
}

export function Insights({ insights, byModel, byProject, quotas = [], initialCache, loadCache = getCacheEfficiency }: InsightsProps) {
  const quotaAnalyses = quotas.flatMap((quota) => quota?.enabled ? quota.analyses : []);
  const [cacheRange, setCacheRange] = useState<CacheRange>(initialCache?.range ?? "7d");
  const [cacheProvider, setCacheProvider] = useState(initialCache?.providerFilter ?? "all");
  const [cache, setCache] = useState<CacheEfficiencyReport | null>(initialCache ?? null);
  const [cacheLoading, setCacheLoading] = useState(!initialCache);
  const [cacheError, setCacheError] = useState<string | null>(null);
  const preserveInitial = useRef(Boolean(initialCache));

  useEffect(() => {
    if (preserveInitial.current) {
      preserveInitial.current = false;
      return;
    }
    let current = true;
    setCacheLoading(true);
    setCacheError(null);
    void loadCache(cacheRange, cacheProvider === "all" ? undefined : cacheProvider)
      .then((next) => { if (current) setCache(next); })
      .catch((reason: unknown) => { if (current) setCacheError(reason instanceof Error ? reason.message : "Cache analytics unavailable."); })
      .finally(() => { if (current) setCacheLoading(false); });
    return () => { current = false; };
  }, [cacheProvider, cacheRange, loadCache]);

  return <div className="page-stack insights-page">
    <section className="insight-lead"><p className="eyebrow">Deterministic analytics</p><h1>What changed, without the guesswork.</h1><p>Every observation below is computed from measured metadata. No LLM-generated narrative is involved.</p></section>
    <CacheEfficiencySection cache={cache} loading={cacheLoading} error={cacheError} range={cacheRange} provider={cacheProvider} onRange={setCacheRange} onProvider={setCacheProvider} />
    {quotaAnalyses.length > 0 ? <section className="panel quota-insights" aria-labelledby="quota-pace-title">
      <div className="panel-heading"><div><h2 id="quota-pace-title">Quota pace</h2><p>Provider-reported percentages · current period · same source device</p></div></div>
      <div className="quota-insight-list">{quotaAnalyses.map((analysis) => <div key={`${analysis.provider}-${analysis.windowKey}`}><div><strong>{analysis.provider === "grok" ? "Grok" : "Claude"} {analysis.label}</strong><small>{analysis.capBearing ? formatQuotaReset(analysis.resetsAt) : "Product allocation · no independent cap ETA"}</small></div><strong>{formatQuotaPercent(analysis.utilizationBps)}</strong><span>{quotaInsightPace(analysis)}</span><small>{quotaInsightProjection(analysis)}</small></div>)}</div>
    </section> : null}
    {insights.length > 0 ? <section className="insight-grid">{insights.map((insight, index) => {
      const Icon = index % 3 === 0 ? TrendingUp : index % 3 === 1 ? FolderKanban : BarChart3;
      return <article className="insight-card" key={insight.id}><span className={`insight-icon tone-${insight.tone}`}><Icon /></span><div><h2>{insight.title}</h2><p>{insight.detail}</p></div></article>;
    })}</section> : null}
    {byModel.length > 0 || byProject.length > 0 ? <section className="insight-rankings"><Ranking title="Model concentration" items={byModel} /><Ranking title="Project concentration" items={byProject} /></section> : null}
  </div>;
}

function CacheEfficiencySection({ cache, loading, error, range, provider, onRange, onProvider }: {
  cache: CacheEfficiencyReport | null; loading: boolean; error: string | null; range: CacheRange; provider: string;
  onRange: (range: CacheRange) => void; onProvider: (provider: string) => void;
}) {
  const summary = cache?.summary;
  const hasCacheCounters = Boolean(summary && (summary.cachedInputTokens > 0 || summary.cacheWriteTokens > 0));
  return <section className="panel cache-efficiency" aria-labelledby="cache-efficiency-title">
    <div className="cache-heading"><div><p className="eyebrow">Cache efficiency</p><h2 id="cache-efficiency-title">Measured input reuse</h2><span>Canonical event-period analytics; this is not a provider cache hit rate.</span></div><div className="cache-filters">
      <label><span className="sr-only">Cache provider</span><select aria-label="Cache provider" value={provider} onChange={(event) => onProvider(event.target.value)}><option value="all">All providers</option>{cache?.availableProviders.map((value) => <option value={value} key={value}>{providerLabel(value)}</option>)}</select></label>
      <label><span className="sr-only">Cache date range</span><select aria-label="Cache date range" value={range} onChange={(event) => onRange(event.target.value as CacheRange)}><option value="today">Today</option><option value="7d">Last 7 days</option><option value="30d">Last 30 days</option><option value="all">All time</option></select></label>
    </div></div>
    {error ? <div className="cache-empty" role="alert">{error}</div> : loading && !summary ? <div className="cache-empty">Loading cache telemetry…</div> : summary?.measuredEventCount === 0 ? <div className="cache-empty">No measured cache activity yet.</div> : !hasCacheCounters ? <div className="cache-empty">Cache telemetry is unavailable for the providers in this range.</div> : summary ? <>
      <div className="cache-metrics"><div className="cache-reuse"><span>Input reuse</span><strong>{formatReuse(summary.reuseShareBps)}</strong><small>{semanticQualifier(summary.semanticCoverage)}</small></div><CacheMetric label="Cache read" value={summary.cachedInputTokens} /><CacheMetric label="Fresh input" value={summary.freshInputTokens} /><CacheMetric label="Cache write" value={summary.cacheWriteTokens} /><div className={`cache-impact ${summary.apiEquivalentCacheImpactUsdMicros !== null && summary.apiEquivalentCacheImpactUsdMicros < 0 ? "negative" : ""}`}><span>API-equivalent cache impact</span><strong>{impactLabel(summary.apiEquivalentCacheImpactUsdMicros)}</strong><small>{pricingQualifier(summary.cachePricingCoverage, summary.apiEquivalentCacheImpactUsdMicros)}</small></div></div>
      {summary.cacheWrite5mTokens > 0 || summary.cacheWrite1hTokens > 0 || (provider !== "grok" && summary.cacheWriteUnspecifiedTokens > 0) ? <div className="cache-write-detail" aria-label="Cache write duration detail"><span>Cache created</span>{summary.cacheWrite5mTokens > 0 ? <small>5-minute <strong>{formatTokens(summary.cacheWrite5mTokens)}</strong></small> : null}{summary.cacheWrite1hTokens > 0 ? <small>1-hour <strong>{formatTokens(summary.cacheWrite1hTokens)}</strong></small> : null}{provider !== "grok" && summary.cacheWriteUnspecifiedTokens > 0 ? <small>Unspecified <strong>{formatTokens(summary.cacheWriteUnspecifiedTokens)}</strong></small> : null}</div> : null}
      <div className="cache-breakdowns"><CacheRanking title="By provider" items={cache.byProvider} /><CacheRanking title="By model" items={cache.byModel} /><CacheRanking title="By project" items={cache.byProject} /></div>
    </> : <div className="cache-empty">Loading cache telemetry…</div>}
  </section>;
}

function CacheMetric({ label, value }: { label: string; value: number | null }) { return <div><span>{label}</span><strong>{value === null ? "Unavailable" : formatTokens(value)}</strong></div>; }

function CacheRanking({ title, items }: { title: string; items: CacheEfficiencyBreakdown[] }) {
  const visible = items.filter((item) => item.cachedInputTokens > 0 || item.cacheWriteTokens > 0).slice(0, 6);
  return <div className="cache-ranking"><h3>{title}</h3>{visible.length === 0 ? <p>No cache activity</p> : visible.map((item) => <div key={item.key}><span><strong>{item.label}</strong><small>{formatTokens(item.cachedInputTokens)} reused</small></span><b>{formatReuse(item.reuseShareBps)}</b></div>)}</div>;
}

function formatReuse(value: number | null): string { return value === null ? "Unavailable" : `${(value / 100).toFixed(1)}%`; }
function semanticQualifier(coverage: CacheEfficiencyReport["summary"]["semanticCoverage"]): string { return coverage === "complete" ? "Share of normalized input context" : coverage === "partial" ? "Partial semantic coverage · known subset" : "Provider semantics unavailable"; }
function impactLabel(value: number | null): string { return value === null ? "Unavailable" : `≈ ${formatUsdMicrosPrecise(Math.abs(value))} ${value >= 0 ? "lower" : "higher"}`; }
function pricingQualifier(coverage: CacheEfficiencyReport["summary"]["cachePricingCoverage"], value: number | null): string { if (coverage === "partial") return "Partial pricing coverage; known subtotal only"; if (coverage === "unavailable") return "No safe cache price comparison available"; return value !== null && value < 0 ? "Cache creation exceeded reuse in this period" : "Versioned API pricing; not subscription savings"; }

function quotaInsightPace(analysis: ProviderQuotaState["analyses"][number]): string {
  if (analysis.stale) return "Previous readings"; if (analysis.status === "limit_reached") return "Limit reached"; if (analysis.status === "gathering") return "Gathering pace data"; if (analysis.status === "no_recent_change") return "No recent change"; if (analysis.status === "stale") return "Previous readings"; if (analysis.recentBurnBpsPerHour === 0) return "No recent change"; return analysis.recentBurnBpsPerHour === null ? "—" : formatQuotaPace(analysis.recentBurnBpsPerHour);
}
function quotaInsightProjection(analysis: ProviderQuotaState["analyses"][number]): string {
  if (!analysis.capBearing) return "Informational only"; if (analysis.projectedBeforeReset === false) return "Projected below limit at reset"; if (analysis.projectedExhaustionAt) return `Projected limit in ~${formatProjectedDuration(analysis.projectedExhaustionAt)}`; if (analysis.stale) return "Current projection suppressed while stale"; return "No defensible projection yet";
}
function Ranking({ title, items }: { title: string; items: BreakdownItem[] }) {
  return <div className="panel ranking-panel"><div className="panel-heading"><div><h2>{title}</h2><p>Measured share of selected period</p></div></div><div className="rank-list">{items.slice(0, 6).map((item, index) => <div key={item.key}><span className="rank-index">{String(index + 1).padStart(2, "0")}</span><div><strong>{item.label}</strong><span style={{ width: `${item.percentage}%` }} /></div><small>{formatTokens(item.tokens)}</small></div>)}</div></div>;
}
