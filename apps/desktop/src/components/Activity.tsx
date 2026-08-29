import { ChevronDown, Filter, Search } from "lucide-react";
import { useMemo, useState } from "react";
import type { ActivityItem } from "../types";
import { activitySourceLabel, formatActivityDate, formatActivityTime, formatTokenDetail, formatTokens, providerLabel } from "../lib/format";
import { ProviderMark } from "./ProviderMark";

interface ActivityProps {
  items: ActivityItem[];
  hasMore?: boolean;
  loadingMore?: boolean;
  onLoadMore?: () => Promise<void>;
}
export function Activity({ items, hasMore = false, loadingMore = false, onLoadMore }: ActivityProps) {
  const [provider, setProvider] = useState("all");
  const [device, setDevice] = useState("all");
  const [project, setProject] = useState("all");
  const [model, setModel] = useState("all");
  const [query, setQuery] = useState("");
  const [expanded, setExpanded] = useState<string | null>(null);

  const options = useMemo(() => ({
    providers: unique(items.map((item) => item.provider)),
    devices: unique(items.map((item) => item.deviceName)),
    projects: unique(items.map((item) => item.projectName).filter((value): value is string => Boolean(value))),
    models: unique(items.map((item) => item.model).filter((value): value is string => Boolean(value))),
  }), [items]);

  const filtered = useMemo(() => items.filter((item) => {
    if (provider !== "all" && item.provider !== provider) return false;
    if (device !== "all" && item.deviceName !== device) return false;
    if (project !== "all" && item.projectName !== project) return false;
    if (model !== "all" && item.model !== model) return false;
    if (query) {
      const haystack = `${item.projectName ?? ""} ${item.model ?? ""} ${item.deviceName} ${providerLabel(item.provider)}`.toLowerCase();
      if (!haystack.includes(query.toLowerCase())) return false;
    }
    return true;
  }), [device, items, model, project, provider, query]);

  const groups = groupByDate(filtered);
  const hasFilters = [provider, device, project, model].some((value) => value !== "all") || query.length > 0;

  return (
    <div className="page-stack activity-page">
      <section className="filter-bar">
        <div className="search-field"><Search /><input aria-label="Search activity" placeholder="Search activity" value={query} onChange={(event) => setQuery(event.target.value)} /></div>
        <FilterSelect label="Provider" value={provider} onChange={setProvider} options={options.providers.map((value) => ({ value, label: providerLabel(value) }))} />
        <FilterSelect label="Device" value={device} onChange={setDevice} options={options.devices.map(asOption)} />
        <FilterSelect label="Project" value={project} onChange={setProject} options={options.projects.map(asOption)} />
        <FilterSelect label="Model" value={model} onChange={setModel} options={options.models.map(asOption)} />
        {hasFilters ? <button className="text-button" type="button" onClick={() => { setProvider("all"); setDevice("all"); setProject("all"); setModel("all"); setQuery(""); }}>Clear</button> : null}
      </section>

      <div className="activity-summary">
        <div><strong>{filtered.length.toLocaleString()}</strong> shown · {items.length.toLocaleString()} loaded</div>
        <span>Prompts and responses are never displayed or stored by ArcMeter.</span>
      </div>

      {groups.length === 0 ? (
        <div className="substantial-empty"><Filter /><h2>No activity matches</h2><p>Adjust your filters or scan local sources again.</p></div>
      ) : (
        <>
          <div className="timeline">
            {groups.map(([date, rows]) => (
              <section className="timeline-day" key={date}>
                <div className="timeline-date"><span>{date}</span><small>{daySummary(rows)}</small></div>
                <div className="timeline-rows">
                  {rows.map((item) => (
                    <div className={`activity-row ${expanded === item.id ? "expanded" : ""}`} key={item.id}>
                      <button type="button" className="activity-main" onClick={() => setExpanded(expanded === item.id ? null : item.id)} aria-expanded={expanded === item.id}>
                        <time>{formatActivityTime(item.occurredAt)}</time>
                        <ProviderMark provider={item.provider} />
                        <div className="activity-identity"><strong>{activitySourceLabel(item.source, item.provider)}</strong><span>{item.measurementKind === "activity_only" ? "Foreground activity" : item.projectName ?? "Unknown project"}</span></div>
                        <div className="activity-model"><span>{item.measurementKind === "activity_only" ? "Token telemetry unavailable" : item.model ?? "Model unavailable"}</span><small>{item.deviceName}</small></div>
                        <div className="activity-tokens"><strong>{item.measurementKind === "activity_only" ? "1 min" : formatTokens(item.totalTokens)}</strong><span>{item.measurementKind.replace("_", " ")}</span></div>
                        <ChevronDown className="activity-chevron" />
                      </button>
                      {expanded === item.id ? (
                        <div className="token-detail">
                          {item.measurementKind === "activity_only" ? <div className="activity-only-detail"><span>Privacy-safe signal</span><strong>One foreground minute; no URL, title, prompt, response, or token count stored.</strong></div> : <>
                            <TokenPart label="Input" value={item.inputTokens} />
                            <TokenPart label="Cached input" value={item.cachedInputTokens} />
                            <TokenPart label="Output" value={item.outputTokens} />
                            <TokenPart label="Reasoning" value={item.reasoningTokens} />
                          </>}
                          <div className="identity-note">Deterministic event <code>{item.id.slice(0, 12)}</code></div>
                        </div>
                      ) : null}
                    </div>
                  ))}
                </div>
              </section>
            ))}
          </div>
          {hasMore && onLoadMore ? <div className="activity-load-more"><button type="button" className="secondary-button" disabled={loadingMore} onClick={() => void onLoadMore()}>{loadingMore ? "Loading older activity…" : "Load older activity"}</button></div> : null}
        </>
      )}
    </div>
  );
}

function FilterSelect({ label, value, onChange, options }: { label: string; value: string; onChange: (value: string) => void; options: { value: string; label: string }[] }) {
  return (
    <label className="select-wrap">
      <span className="sr-only">{label}</span>
      <select value={value} onChange={(event) => onChange(event.target.value)}>
        <option value="all">{label}</option>
        {options.map((option) => <option key={option.value} value={option.value}>{option.label}</option>)}
      </select>
      <ChevronDown />
    </label>
  );
}

function TokenPart({ label, value }: { label: string; value: number }) {
  return <div><span>{label}</span><strong>{formatTokenDetail(value)}</strong></div>;
}

function unique(values: string[]): string[] {
  return [...new Set(values)].sort((a, b) => a.localeCompare(b));
}

function asOption(value: string) {
  return { value, label: value };
}

function groupByDate(items: ActivityItem[]): [string, ActivityItem[]][] {
  const groups = new Map<string, ActivityItem[]>();
  for (const item of items) {
    const key = formatActivityDate(item.occurredAt);
    const group = groups.get(key) ?? [];
    group.push(item);
    groups.set(key, group);
  }
  return [...groups];
}

function daySummary(items: ActivityItem[]): string {
  const measured = items.reduce((sum, item) => sum + item.totalTokens, 0);
  const minutes = items.filter((item) => item.measurementKind === "activity_only").length;
  if (measured > 0 && minutes > 0) return `${formatTokens(measured)} measured · ${minutes} active min`;
  if (minutes > 0) return `${minutes} active min`;
  return `${formatTokens(measured)} measured`;
}
