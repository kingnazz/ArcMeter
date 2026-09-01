import { BarChart3, CircleGauge, FolderKanban, TrendingUp } from "lucide-react";
import type { BreakdownItem, Insight, ProviderQuotaState } from "../types";
import { formatProjectedDuration, formatQuotaPace, formatQuotaPercent, formatQuotaReset, formatTokens } from "../lib/format";

export function Insights({ insights, byModel, byProject, quotas = [] }: { insights: Insight[]; byModel: BreakdownItem[]; byProject: BreakdownItem[]; quotas?: (ProviderQuotaState | null)[] }) {
  const quotaAnalyses = quotas.flatMap((quota) => quota?.enabled ? quota.analyses : []);
  if (insights.length === 0 && quotaAnalyses.length === 0) {
    return (
      <div className="substantial-empty insights-empty">
        <CircleGauge />
        <h2>Insights need a reliable baseline</h2>
        <p>ArcMeter will surface deterministic comparisons once measured usage exists across enough comparable periods.</p>
      </div>
    );
  }
  return (
    <div className="page-stack insights-page">
      <section className="insight-lead">
        <p className="eyebrow">Deterministic analytics</p>
        <h1>What changed, without the guesswork.</h1>
        <p>Every observation below is computed from measured metadata. No LLM-generated narrative is involved.</p>
      </section>
      {quotaAnalyses.length > 0 ? (
        <section className="panel quota-insights" aria-labelledby="quota-pace-title">
          <div className="panel-heading"><div><h2 id="quota-pace-title">Quota pace</h2><p>Provider-reported percentages · current period · same source device</p></div></div>
          <div className="quota-insight-list">
            {quotaAnalyses.map((analysis) => (
              <div key={`${analysis.provider}-${analysis.windowKey}`}>
                <div><strong>{analysis.provider === "grok" ? "Grok" : "Claude"} {analysis.label}</strong><small>{analysis.capBearing ? formatQuotaReset(analysis.resetsAt) : "Product allocation · no independent cap ETA"}</small></div>
                <strong>{formatQuotaPercent(analysis.utilizationBps)}</strong>
                <span>{quotaInsightPace(analysis)}</span>
                <small>{quotaInsightProjection(analysis)}</small>
              </div>
            ))}
          </div>
        </section>
      ) : null}
      {insights.length > 0 ? <section className="insight-grid">
        {insights.map((insight, index) => {
          const Icon = index % 3 === 0 ? TrendingUp : index % 3 === 1 ? FolderKanban : BarChart3;
          return (
            <article className="insight-card" key={insight.id}>
              <span className={`insight-icon tone-${insight.tone}`}><Icon /></span>
              <div><h2>{insight.title}</h2><p>{insight.detail}</p></div>
            </article>
          );
        })}
      </section> : null}
      {byModel.length > 0 || byProject.length > 0 ? <section className="insight-rankings">
        <Ranking title="Model concentration" items={byModel} />
        <Ranking title="Project concentration" items={byProject} />
      </section> : null}
    </div>
  );
}

function quotaInsightPace(analysis: ProviderQuotaState["analyses"][number]): string {
  if (analysis.stale) return "Previous readings";
  if (analysis.status === "limit_reached") return "Limit reached";
  if (analysis.status === "gathering") return "Gathering pace data";
  if (analysis.status === "no_recent_change") return "No recent change";
  if (analysis.status === "stale") return "Previous readings";
  if (analysis.recentBurnBpsPerHour === 0) return "No recent change";
  return analysis.recentBurnBpsPerHour === null ? "—" : formatQuotaPace(analysis.recentBurnBpsPerHour);
}

function quotaInsightProjection(analysis: ProviderQuotaState["analyses"][number]): string {
  if (!analysis.capBearing) return "Informational only";
  if (analysis.projectedBeforeReset === false) return "Projected below limit at reset";
  if (analysis.projectedExhaustionAt) return `Projected limit in ~${formatProjectedDuration(analysis.projectedExhaustionAt)}`;
  if (analysis.stale) return "Current projection suppressed while stale";
  return "No defensible projection yet";
}
function Ranking({ title, items }: { title: string; items: BreakdownItem[] }) {
  return (
    <div className="panel ranking-panel">
      <div className="panel-heading"><div><h2>{title}</h2><p>Measured share of selected period</p></div></div>
      <div className="rank-list">
        {items.slice(0, 6).map((item, index) => (
          <div key={item.key}>
            <span className="rank-index">{String(index + 1).padStart(2, "0")}</span>
            <div><strong>{item.label}</strong><span style={{ width: `${item.percentage}%` }} /></div>
            <small>{formatTokens(item.tokens)}</small>
          </div>
        ))}
      </div>
    </div>
  );
}
