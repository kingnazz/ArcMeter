import { ChevronDown, Filter, Search, X } from "lucide-react";
import { useEffect, useMemo, useRef, useState, type ReactNode } from "react";
import type { SessionDetail, SessionPage, SessionQuery, SessionSummary } from "../types";
import { getSessionDetail, getSessionPage } from "../lib/api";
import { formatActivityDate, formatActivityTime, formatSessionDuration, formatTokenDetail, formatTokens, formatUsdMicrosPrecise, formatUsdTicks, providerLabel } from "../lib/format";
import { ProviderMark } from "./ProviderMark";

const emptyPage: SessionPage = {
  sessions: [],
  totalCount: 0,
  stats: { sessionCount: 0, totalTokens: 0, estimatedApiValueUsdMicros: null },
  hasMore: false,
};

interface SessionsProps {
  initialPage?: SessionPage;
  loadPage?: (query: SessionQuery) => Promise<SessionPage>;
  loadDetail?: (session: SessionSummary, limit?: number, offset?: number) => Promise<SessionDetail>;
}

export function Sessions({ initialPage, loadPage = getSessionPage, loadDetail = (session, limit, offset) => getSessionDetail(session, limit, offset) }: SessionsProps) {
  const [range, setRange] = useState<SessionQuery["range"]>("30d");
  const [provider, setProvider] = useState("all");
  const [search, setSearch] = useState("");
  const [sort, setSort] = useState<NonNullable<SessionQuery["sort"]>>("recent");
  const [page, setPage] = useState<SessionPage>(initialPage ?? emptyPage);
  const [loading, setLoading] = useState(!initialPage);
  const [loadingMore, setLoadingMore] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [selected, setSelected] = useState<SessionSummary | null>(null);
  const [detail, setDetail] = useState<SessionDetail | null>(null);
  const [detailLoading, setDetailLoading] = useState(false);
  const [eventsLoading, setEventsLoading] = useState(false);
  const preserveInitial = useRef(Boolean(initialPage));
  const selectionKey = useRef<string | null>(null);

  const providers = useMemo(
    () => [...new Set(["codex", "claude", "grok", "gemini", ...page.sessions.map((session) => session.provider)])].sort((left, right) => providerLabel(left).localeCompare(providerLabel(right))),
    [page.sessions],
  );
  const hasFilters = provider !== "all" || range !== "30d" || search.length > 0 || sort !== "recent";

  useEffect(() => {
    if (preserveInitial.current) {
      preserveInitial.current = false;
      return;
    }
    let current = true;
    setLoading(true);
    setError(null);
    void loadPage(queryFor(range, provider, search, sort, 0)).then((next) => {
      if (!current) return;
      setPage(next);
      setSelected(null);
      setDetail(null);
    }).catch((reason: unknown) => {
      if (current) setError(messageFor(reason));
    }).finally(() => {
      if (current) setLoading(false);
    });
    return () => { current = false; };
  }, [loadPage, provider, range, search, sort]);

  async function loadMore() {
    if (loadingMore || !page.hasMore) return;
    setLoadingMore(true);
    try {
      const next = await loadPage(queryFor(range, provider, search, sort, page.sessions.length));
      setPage((current) => ({
        ...next,
        sessions: [...current.sessions, ...next.sessions.filter((item) => !current.sessions.some((existing) => existing.sessionKey === item.sessionKey))],
      }));
    } catch (reason) {
      setError(messageFor(reason));
    } finally {
      setLoadingMore(false);
    }
  }

  async function selectSession(session: SessionSummary) {
    selectionKey.current = session.sessionKey;
    setSelected(session);
    setDetail(null);
    setDetailLoading(true);
    try {
      const next = await loadDetail(session);
      if (selectionKey.current === session.sessionKey) setDetail(next);
    } catch (reason) {
      if (selectionKey.current === session.sessionKey) setError(messageFor(reason));
    } finally {
      if (selectionKey.current === session.sessionKey) setDetailLoading(false);
    }
  }

  async function loadMoreEvents() {
    if (!selected || !detail || eventsLoading || !detail.eventsHasMore) return;
    setEventsLoading(true);
    try {
      const next = await loadDetail(selected, 100, detail.events.length);
      if (selectionKey.current !== selected.sessionKey) return;
      setDetail((current) => current ? {
        ...next,
        events: [...current.events, ...next.events],
      } : current);
    } catch (reason) {
      setError(messageFor(reason));
    } finally {
      setEventsLoading(false);
    }
  }

  function clearFilters() {
    setRange("30d");
    setProvider("all");
    setSearch("");
    setSort("recent");
  }

  const groups = groupSessions(page.sessions);
  return (
    <div className="page-stack session-page">
      <section className="filter-bar" aria-label="Session filters">
        <div className="search-field"><Search /><input aria-label="Search projects" placeholder="Search projects, models, or devices" value={search} onChange={(event) => setSearch(event.target.value)} /></div>
        <SessionSelect label="Provider" value={provider} onChange={setProvider} options={providers.map((value) => ({ value, label: providerLabel(value) }))} />
        <SessionSelect label="Date range" value={range} onChange={(value) => setRange(value as SessionQuery["range"])} options={[{ value: "today", label: "Today" }, { value: "7d", label: "Last 7 days" }, { value: "30d", label: "Last 30 days" }, { value: "all", label: "All time" }]} />
        <SessionSelect label="Sort sessions" value={sort} onChange={(value) => setSort(value as NonNullable<SessionQuery["sort"]>)} options={[{ value: "recent", label: "Most recent" }, { value: "tokens", label: "Most tokens" }, { value: "value", label: "Highest value" }, { value: "duration", label: "Longest" }]} />
        {hasFilters ? <button className="text-button" type="button" onClick={clearFilters}>Clear</button> : null}
      </section>

      <section className="session-summary-strip" aria-label="Session totals">
        <div><span>Sessions</span><strong>{page.stats.sessionCount.toLocaleString()}</strong></div>
        <div><span>Measured tokens</span><strong>{formatTokens(page.stats.totalTokens)}</strong></div>
        <div><span>API-equivalent value</span><strong>{formatUsdMicrosPrecise(page.stats.estimatedApiValueUsdMicros)}</strong></div>
        <p>Only canonical measured events are grouped. Activity-only and estimated records are excluded.</p>
      </section>

      {error ? <div className="session-inline-error" role="alert">{error}</div> : null}
      {loading ? <div className="session-loading">Loading measured sessions…</div> : null}
      {!loading && groups.length === 0 ? (
        <div className="substantial-empty"><Filter /><h2>{hasFilters ? "No sessions match these filters" : "No measured sessions yet"}</h2><p>{hasFilters ? "Try a broader date, provider, or project search." : "ArcMeter will group authoritative measured events here after it finds local CLI telemetry."}</p></div>
      ) : null}
      {!loading && groups.length > 0 ? (
        <div className={selected ? "sessions-layout has-detail" : "sessions-layout"}>
          <div className="session-groups">
            {groups.map(([label, sessions]) => (
              <section className="session-group" key={label}>
                <div className="session-group-heading"><h2>{label}</h2><span>{sessions.length} session{sessions.length === 1 ? "" : "s"}</span></div>
                <div className="session-list">
                  {sessions.map((session) => <SessionRow key={session.sessionKey} session={session} selected={selected?.sessionKey === session.sessionKey} onSelect={() => void selectSession(session)} />)}
                </div>
              </section>
            ))}
            {page.hasMore ? <div className="session-load-more"><button type="button" className="secondary-button" disabled={loadingMore} onClick={() => void loadMore()}>{loadingMore ? "Loading sessions…" : "Load more sessions"}</button></div> : null}
          </div>
          {selected ? <SessionDetailPanel session={selected} detail={detail} loading={detailLoading} loadingMoreEvents={eventsLoading} onClose={() => { selectionKey.current = null; setSelected(null); setDetail(null); }} onLoadMoreEvents={() => void loadMoreEvents()} /> : null}
        </div>
      ) : null}
    </div>
  );
}

function SessionRow({ session, selected, onSelect }: { session: SessionSummary; selected: boolean; onSelect: () => void }) {
  const deviceLabel = session.deviceCount > 1 ? `${session.deviceCount} devices` : session.primaryDeviceName;
  return (
    <button type="button" className={selected ? "session-row selected" : "session-row"} onClick={onSelect} aria-pressed={selected}>
      <ProviderMark provider={session.provider} />
      <div className="session-row-main">
        <strong>{session.projectName}</strong>
        <span>{sourceLabel(session.source, session.provider)} · {session.primaryModel}{session.modelCount > 1 ? ` +${session.modelCount - 1}` : ""}</span>
      </div>
      <div className="session-row-meta"><span>{formatActivityDate(session.lastActivityAt)} · {formatActivityTime(session.startedAt)}–{formatActivityTime(session.lastActivityAt)}</span><small>{formatSessionDuration(session.durationSeconds)} · {deviceLabel}</small></div>
      <div className="session-row-metrics"><strong>{formatTokens(session.totalTokens)}</strong><span>{valueLabel(session)}</span></div>
    </button>
  );
}

function SessionDetailPanel({ session, detail, loading, loadingMoreEvents, onClose, onLoadMoreEvents }: { session: SessionSummary; detail: SessionDetail | null; loading: boolean; loadingMoreEvents: boolean; onClose: () => void; onLoadMoreEvents: () => void }) {
  const shown = detail?.session ?? session;
  return (
    <aside className="session-detail" aria-label="Session detail">
      <header className="session-detail-header">
        <div><p className="eyebrow">Measured session</p><h2>{shown.projectName}</h2><span>{sourceLabel(shown.source, shown.provider)} · {formatActivityDate(shown.lastActivityAt)}</span></div>
        <button className="icon-button" type="button" onClick={onClose} aria-label="Close session detail"><X /></button>
      </header>
      {loading ? <div className="session-detail-loading">Loading session detail…</div> : detail ? <>
        <div className="session-detail-overview"><div><span>Duration</span><strong>{formatSessionDuration(shown.durationSeconds)}</strong></div><div><span>Events</span><strong>{shown.eventCount.toLocaleString()}</strong></div><div><span>Devices</span><strong>{shown.deviceCount.toLocaleString()}</strong></div></div>
        <DetailSection title="Models"><div className="session-model-list">{detail.models.map((model) => <div key={model.model}><span>{model.model}</span><small>{model.eventCount} events</small><strong>{formatTokens(model.tokens)}</strong></div>)}</div></DetailSection>
        <DetailSection title="Token composition"><div className="session-token-grid"><TokenDetail label={shown.provider === "grok" ? "Input (cache included)" : shown.provider === "claude" ? "Fresh input (cache separate)" : "Input"} value={shown.inputTokens} /><TokenDetail label={shown.provider === "claude" ? "Cache read" : "Cached input"} value={shown.cachedInputTokens} /><TokenDetail label="Cache write" value={shown.cacheWriteTokens} /><TokenDetail label="Cache write (5m)" value={shown.cacheWrite5mTokens} /><TokenDetail label="Cache write (1h)" value={shown.cacheWrite1hTokens} /><TokenDetail label="Output" value={shown.outputTokens} /><TokenDetail label={shown.provider === "claude" || shown.provider === "grok" ? "Reasoning (included in output)" : "Reasoning"} value={shown.reasoningTokens} /></div></DetailSection>
        <DetailSection title="Cache"><div className="session-cache-summary"><div className="session-cache-reuse"><span>Input reuse</span><strong>{formatReuse(detail.cache.reuseShareBps)}</strong><small>{semanticDescription(detail.cache.semanticCoverage)}</small></div><div className="session-token-grid"><TokenDetail label="Fresh input" value={detail.cache.freshInputTokens} /><TokenDetail label="Cache read" value={detail.cache.cachedInputTokens} /><TokenDetail label="Cache write" value={detail.cache.cacheWriteTokens} />{detail.cache.cacheWrite5mTokens > 0 ? <TokenDetail label="5m writes" value={detail.cache.cacheWrite5mTokens} /> : null}{detail.cache.cacheWrite1hTokens > 0 ? <TokenDetail label="1h writes" value={detail.cache.cacheWrite1hTokens} /> : null}{shown.provider !== "grok" && detail.cache.cacheWriteUnspecifiedTokens > 0 ? <TokenDetail label="Unspecified writes" value={detail.cache.cacheWriteUnspecifiedTokens} /> : null}</div></div></DetailSection>
        <DetailSection title="Value"><div className="session-value-list"><div><span>API-equivalent value</span><strong>{formatUsdMicrosPrecise(shown.estimatedApiValueUsdMicros)}</strong><small>{coverageDescription(shown.pricingCoverage)}</small></div>{shown.nativeCostUsdTicks !== null ? <div><span>Recorded provider cost</span><strong>{formatUsdTicks(shown.nativeCostUsdTicks)}</strong><small>Provider-recorded; not an ArcMeter estimate.</small></div> : null}</div></DetailSection>
        <DetailSection title="Devices"><div className="session-device-list">{detail.devices.map((device) => <span key={device}>{device}</span>)}</div></DetailSection>
        <DetailSection title="Event timeline"><div className="session-event-list">{detail.events.map((event, index) => <div key={`${event.occurredAt}-${event.model}-${index}`}><time>{formatActivityTime(event.occurredAt)}</time><span>{event.model}</span><strong>{formatTokens(event.totalTokens)}</strong>{event.estimatedApiValueUsdMicros !== null ? <small>{formatUsdMicrosPrecise(event.estimatedApiValueUsdMicros)}</small> : null}</div>)}</div>{detail.eventsHasMore ? <button type="button" className="text-button" disabled={loadingMoreEvents} onClick={onLoadMoreEvents}>{loadingMoreEvents ? "Loading events…" : "Load more events"}</button> : null}</DetailSection>
      </> : null}
    </aside>
  );
}

function SessionSelect({ label, value, onChange, options }: { label: string; value: string; onChange: (value: string) => void; options: { value: string; label: string }[] }) {
  return <label className="select-wrap"><span className="sr-only">{label}</span><select aria-label={label} value={value} onChange={(event) => onChange(event.target.value)}><option value="all">{label}</option>{options.map((option) => <option value={option.value} key={option.value}>{option.label}</option>)}</select><ChevronDown /></label>;
}

function DetailSection({ title, children }: { title: string; children: ReactNode }) {
  return <section className="session-detail-section"><h3>{title}</h3>{children}</section>;
}

function TokenDetail({ label, value }: { label: string; value: number | null }) {
  return <div><span>{label}</span><strong>{value === null ? "Unavailable" : formatTokenDetail(value)}</strong></div>;
}

function groupSessions(sessions: SessionSummary[]): [string, SessionSummary[]][] {
  const groups = new Map<string, SessionSummary[]>();
  for (const session of sessions) {
    const label = sessionGroupLabel(session.lastActivityAt);
    const items = groups.get(label) ?? [];
    items.push(session);
    groups.set(label, items);
  }
  return [...groups];
}

function sessionGroupLabel(value: string): string {
  const date = new Date(value);
  const now = new Date();
  const startOfToday = new Date(now.getFullYear(), now.getMonth(), now.getDate());
  const startOfDate = new Date(date.getFullYear(), date.getMonth(), date.getDate());
  const daysAgo = Math.round((startOfToday.getTime() - startOfDate.getTime()) / 86_400_000);
  if (daysAgo === 0) return "Today";
  if (daysAgo === 1) return "Yesterday";
  if (daysAgo >= 2 && daysAgo < 7) return "Earlier this week";
  return "Older";
}

function sourceLabel(source: string, provider: string): string {
  const labels: Record<string, string> = { codex_cli: "Codex", claude_code: "Claude Code", grok_build: "Grok Build", gemini_cli: "Gemini CLI" };
  return labels[source] ?? providerLabel(provider);
}

function valueLabel(session: SessionSummary): string {
  if (session.estimatedApiValueUsdMicros === null) return "Pricing unavailable";
  const qualifier = session.pricingCoverage === "complete" ? "Complete pricing" : "Partial pricing coverage";
  return `${formatUsdMicrosPrecise(session.estimatedApiValueUsdMicros)} · ${qualifier}`;
}

function coverageDescription(coverage: SessionSummary["pricingCoverage"]): string {
  return coverage === "complete" ? "Exact pricing covers every token-bearing event." : coverage === "partial" ? "Partial pricing coverage; priced subtotal excludes unavailable components." : "No safe pricing was available for this session.";
}

function formatReuse(value: number | null): string {
  return value === null ? "Unavailable" : `${(value / 100).toFixed(1)}%`;
}

function semanticDescription(coverage: SessionDetail["cache"]["semanticCoverage"]): string {
  return coverage === "complete" ? "Authoritative normalized input semantics." : coverage === "partial" ? "Partial semantic coverage; ratio uses the known subset." : "Reuse share unavailable for this source.";
}

function queryFor(range: SessionQuery["range"], provider: string, search: string, sort: NonNullable<SessionQuery["sort"]>, offset: number): SessionQuery {
  return { range, provider: provider === "all" ? undefined : provider, search: search.trim() || undefined, sort, limit: 50, offset };
}

function messageFor(reason: unknown): string {
  return reason instanceof Error ? reason.message : "ArcMeter could not load measured sessions.";
}
