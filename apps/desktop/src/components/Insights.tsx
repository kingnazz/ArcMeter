import { BarChart3, CircleGauge, FolderKanban, TrendingUp } from "lucide-react";
import type { BreakdownItem, Insight } from "../types";
import { formatTokens } from "../lib/format";

export function Insights({ insights, byModel, byProject }: { insights: Insight[]; byModel: BreakdownItem[]; byProject: BreakdownItem[] }) {
  if (insights.length === 0) {
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
      <section className="insight-grid">
        {insights.map((insight, index) => {
          const Icon = index % 3 === 0 ? TrendingUp : index % 3 === 1 ? FolderKanban : BarChart3;
          return (
            <article className="insight-card" key={insight.id}>
              <span className={`insight-icon tone-${insight.tone}`}><Icon /></span>
              <div><h2>{insight.title}</h2><p>{insight.detail}</p></div>
            </article>
          );
        })}
      </section>
      <section className="insight-rankings">
        <Ranking title="Model concentration" items={byModel} />
        <Ranking title="Project concentration" items={byProject} />
      </section>
    </div>
  );
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
