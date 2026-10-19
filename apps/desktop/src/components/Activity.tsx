import { ChevronDown, Filter, Search } from "lucide-react";
import { useMemo, useState } from "react";
import type { ActivityItem } from "../types";
import { activitySourceLabel, formatActivityDate, formatActivityTime, formatTokenDetail, formatTokens, formatUsdMicros, formatUsdTicks, isLocalProvider, providerLabel } from "../lib/format";
import { ProviderMark } from "./ProviderMark";

interface ActivityProps {
  items: ActivityItem[];
  hasMore?: boolean;
  loadingMore?: boolean;
  onLoadMore?: () => Promise<void>;
}
export function Activity({ items, hasMore = false, loadingMore = false, onLoadMore }: ActivityProps) {
  const [provider, setProvider] = useState("all");
  const [location, setLocation] = useState("all");
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
    if (location === "local" && !isLocalProvider(item.provider)) return false;
    if (location === "cloud" && isLocalProvider(item.provider)) return false;
    if (provider !== "all" && item.provider !== provider) return false;
    if (device !== "all" && item.deviceName !== device) return false;
    if (project !== "all" && item.projectName !== project) return false;
    if (model !== "all" && item.model !== model) return false;
    if (query) {
      const haystack = `${item.projectName ?? ""} ${item.model ?? ""} ${item.deviceName} ${providerLabel(item.provider)}`.toLowerCase();
      if (!haystack.includes(query.toLowerCase())) return false;
    }
    return true;
  }), [device, items, model, project, provider, query, location]);

  const groups = groupByDate(filtered);
  const hasFilters = [provider, device, project, model, location].some((value) => value !== "all") || query.length > 0;

  return (
    <div className="page-stack activity-page">
      <section className="filter-bar">
        <div className="search-field"><Search /><input aria-label="Search activity" placeholder="Search activity" value={query} onChange={(event) => setQuery(event.target.value)} /></div>
        <FilterSelect label="Provider" value={provider} onChange={setProvider} options={options.providers.map((value) => ({ value, label: providerLabel(value) }))} />
        <FilterSelect label="Location" value={location} onChange={setLocation} options={[{ value: "local", label: "Local runtimes" }, { value: "cloud", label: "Cloud tools" }]} />
        <FilterSelect label="Device" value={device} onChange={setDevice} options={options.devices.map(asOption)} />
        <FilterSelect label="Project" value={project} onChange={setProject} options={options.projects.map(asOption)} />
        <FilterSelect label="Model" value={model} onChange={setModel} options={options.models.map(asOption)} />
        {hasFilters ? <button className="text-button" type="button" onClick={() => { setProvider("all"); setDevice("all"); setProject("all"); setModel("all"); setQuery(""); setLocation("all"); }}>Clear</button> : null}
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
                        <div className="activity-identity"><strong>{activitySourceLabel(item.source, item.provider)}</strong><span>{isLocalProvider(item.provider) ? <b className="local-badge">Local</b> : null}{item.measurementKind === "runtime_status" ? "Runtime observation" : item.measurementKind === "activity_only" ? "Foreground activity" : item.source === "omlx_usage_hourly" ? "Hour beginning at shown time" : item.projectName ?? "Unknown project"}</span></div>
                        <div className="activity-model"><span>{item.measurementKind === "activity_only" ? "Token telemetry unavailable" : item.model ?? (item.measurementKind === "runtime_status" ? "Runtime status change" : "Model unavailable")}</span><small>{item.deviceName}</small></div>
                        <div className="activity-tokens"><strong>{item.measurementKind === "runtime_status" ? "—" : item.measurementKind === "activity_only" ? "1 min" : formatTokens(item.totalTokens)}</strong><span>{item.measurementKind === "runtime_status" ? "state change" : item.source === "omlx_usage_hourly" ? "measured / hour" : item.measurementKind.replace("_", " ")}</span></div>
                        <ChevronDown className="activity-chevron" />
                      </button>
                      {expanded === item.id ? (
                        <div className="token-detail">
                          {item.measurementKind === "runtime_status" ? <div className="activity-only-detail"><span>Runtime observation</span><strong>Observed server or model state change. No request, token, session or active-time measurement.</strong></div> : item.measurementKind === "activity_only" ? <div className="activity-only-detail"><span>Privacy-safe signal</span><strong>One foreground minute; no URL, title, prompt, response, or token count stored.</strong></div> : <>
                            <TokenPart label={item.provider === "grok" ? "Input (cache included)" : item.provider === "claude" ? "Fresh input (cache separate)" : "Input"} value={item.inputTokens} />
                            <TokenPart label={item.provider === "claude" ? "Cache read" : "Cached input"} value={item.cachedInputTokens} />
                            {item.cacheWriteTokens > 0 ? <TokenPart label={item.provider === "claude" ? "Cache write total" : "Cache write"} value={item.cacheWriteTokens} /> : null}
                            {item.cacheWrite5mTokens > 0 ? <TokenPart label="Cache write (5m)" value={item.cacheWrite5mTokens} /> : null}
                            {item.cacheWrite1hTokens > 0 ? <TokenPart label="Cache write (1h)" value={item.cacheWrite1hTokens} /> : null}
                            <TokenPart label="Output" value={item.outputTokens} />
                            <TokenPart label={item.provider === "grok" || item.provider === "claude" ? "Reasoning (included in output)" : "Reasoning"} value={item.reasoningTokens} />
                            {item.nativeCostUsdTicks !== null ? <CostPart label="Recorded provider cost" value={formatUsdTicks(item.nativeCostUsdTicks)} /> : null}
                            {item.estimatedApiValueUsdMicros !== null ? <CostPart label="Estimated API-equivalent value" value={formatUsdMicros(item.estimatedApiValueUsdMicros)} /> : null}
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

function CostPart({ label, value }: { label: string; value: string }) {
  return <div><span>{label}</span><strong>{value}</strong></div>;
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
  const measured = items.filter((item) => item.measurementKind === "measured").reduce((sum, item) => sum + item.totalTokens, 0);
  const minutes = items.filter((item) => item.measurementKind === "activity_only").length;
  const observations = items.filter((item) => item.measurementKind === "runtime_status").length;
  return [measured > 0 ? `${formatTokens(measured)} measured` : "", minutes ? `${minutes} active min` : "", observations ? `${observations} runtime observations` : ""].filter(Boolean).join(" · ") || "No measured usage";
}
