import { ArrowUpRight, Cpu, Radio, RefreshCw, Server, ShieldCheck } from "lucide-react";
import { useState } from "react";
import type { LocalModelsSnapshot, LocalRuntime, WorkloadSummary } from "../types";
import { formatRelativeTime, formatTokens, providerLabel } from "../lib/format";
import { configureLocalRuntime, testLocalRuntime } from "../lib/api";
import { ProviderMark } from "./ProviderMark";

export function RuntimeStatus({ runtime }: { runtime: LocalRuntime }) {
  return <span className={`runtime-status ${runtime.status === "healthy" ? "online" : ""}`}><i />{!runtime.enabled ? "Disabled" : runtime.status === "healthy" ? "API reachable" : runtime.status === "stale" ? "Stale" : runtime.status === "not_detected" ? "Not detected" : "Unavailable"}</span>;
}

export function LocalOverview({ local, workload, onOpen }: { local: LocalModelsSnapshot; workload: WorkloadSummary; onOpen?: () => void }) {
  const running = local.runtimes.flatMap((r) => r.enabled && r.status === "healthy" ? r.models.filter((m) => m.loaded).map((m) => ({ runtime: r, model: m })) : []);
  const first = running[0];
  const total = workload.localTokens + workload.cloudTokens;
  const share = total > 0 ? workload.localTokens / total * 100 : 0;
  return <section className="local-overview" aria-label="Local and cloud usage">
    <div className="local-now"><Cpu /><div><span className="eyebrow">Local · running now</span><strong title={first?.model.id}>{first?.model.name ?? "No loaded local model detected"}</strong><small>{first ? `${providerLabel(first.runtime.provider)} · ${running.length > 1 ? `${running.length} models loaded` : "API reachable"}` : "Automatic discovery · oMLX, Ollama, LM Studio"}</small></div>{onOpen ? <button type="button" aria-label="Open Local Models" onClick={onOpen}><ArrowUpRight /></button> : null}</div>
    <div className="workload-split"><div><span>Measured workload</span><strong>{total ? `${share.toFixed(0)}% local` : "Awaiting measured usage"}</strong></div><div className="split-track" aria-label={`${formatTokens(workload.localTokens)} local tokens, ${formatTokens(workload.cloudTokens)} cloud tool tokens`}><i style={{ width: `${share}%` }} /></div><div className="split-legend"><span><i />Local <b>{formatTokens(workload.localTokens)}</b></span><span><i />Cloud tools <b>{formatTokens(workload.cloudTokens)}</b></span></div><small>Selected period · measured tokens only</small></div>
  </section>;
}

export function LocalModels({ data, scanning, onScan, onSettings }: { data: LocalModelsSnapshot; scanning: boolean; onScan: () => void; onSettings: () => void }) {
  const running = data.runtimes.flatMap((runtime) => runtime.status === "healthy" && runtime.enabled ? runtime.models.filter((m) => m.loaded).map((model) => ({runtime, model})) : []);
  return <div className="page-stack local-models-page">
    <div className="local-toolbar"><p><ShieldCheck />Passive discovery · metadata stays on this computer</p><button type="button" className="secondary-button" disabled={scanning} onClick={onScan}><RefreshCw className={scanning ? "spin" : ""} />{scanning ? "Discovering…" : "Discover now"}</button></div>
    <section className="running-workspace" aria-labelledby="running-title">
      <div className="running-models"><p className="eyebrow"><Radio /> Runtime inventory</p><h2 id="running-title">Running now <span>{running.length}</span></h2>
        {running.length ? running.map(({runtime,model}) => <article className="running-model" key={`${runtime.provider}:${model.id}`}><ProviderMark provider={runtime.provider} /><div><h3>{model.name}</h3><p>{providerLabel(runtime.provider)} <span className="local-badge">Local</span></p><small>Last accessed {model.lastActivityAt ? formatRelativeTime(model.lastActivityAt) : "not reported"}</small></div><span className="runtime-status online"><i />Loaded</span></article>) : <div className="runtime-empty"><Cpu /><h3>No loaded model detected</h3><p>Start a local runtime and load a model. ArcMeter checks automatically every minute.</p><button type="button" className="text-button" onClick={onSettings}>Configure endpoints <ArrowUpRight /></button></div>}
        <p className="local-qualifier">Loaded state shows availability. It does not imply inference or token usage.</p>
      </div>
      <div className="local-today"><p className="eyebrow">Today · this computer</p><dl><div><dt>Measured local tokens</dt><dd>{data.measuredTokensToday ? formatTokens(data.measuredTokensToday) : "—"}</dd></div><div><dt>oMLX requests</dt><dd>{data.requestsToday === null ? "—" : data.requestsToday.toLocaleString()}</dd></div><div><dt>Runtime observations</dt><dd>{data.observationsToday.toLocaleString()}</dd></div><div><dt>Sessions / active time</dt><dd className="unknown-value">Not reported</dd></div></dl><small>{data.historyStatus === "available" ? "oMLX hourly history · cached tokens included in input. Partial current hour; other runtimes have no passive token history." : "Token history is unavailable. Runtime discovery never estimates tokens or requests."}</small></div>
    </section>
    <section className="panel runtime-inventory"><div className="panel-heading"><div><h2>Local runtimes</h2><p>Provider-reported inventory and loaded state</p></div><button className="text-button" type="button" onClick={onSettings}>Manage endpoints <ArrowUpRight /></button></div>
      {data.runtimes.map((runtime) => <RuntimeInventory runtime={runtime} key={runtime.provider} />)}
    </section>
    <p className="local-qualifier"><ShieldCheck />oMLX hourly usage joins Overview, Activity, Insights and cache analytics. Inventory, endpoints and runtime observations stay local.</p>
  </div>;
}

function RuntimeInventory({ runtime }: { runtime: LocalRuntime }) {
  const [expanded, setExpanded] = useState(false);
  const models = expanded ? runtime.models : runtime.models.slice(0, 4);
  return <article className="runtime-entry"><div className="runtime-heading"><ProviderMark provider={runtime.provider} /><div><h3>{providerLabel(runtime.provider)}</h3><code>{runtime.endpoint}</code></div><span className="runtime-mode">{runtime.automatic ? "Automatic" : "Custom endpoint"}</span><RuntimeStatus runtime={runtime} /></div>
    <p className="runtime-diagnostic">{runtime.diagnostic}{runtime.checkedAt ? ` · Checked ${formatRelativeTime(runtime.checkedAt)}` : ""}{runtime.detectedAt ? ` · Last detected ${formatRelativeTime(runtime.detectedAt)}` : ""}</p>
    {models.length ? <div className="model-table" role="table" aria-label={`${providerLabel(runtime.provider)} models`}><div className="model-table-head" role="row"><span role="columnheader">Model / first seen</span><span role="columnheader">State / last seen</span><span role="columnheader">Runtime context</span><span role="columnheader">Quantization</span><span role="columnheader">Parameters / size</span></div>{models.map((m) => <div className="model-table-row" role="row" key={m.id}><span role="cell"><strong title={m.id}>{m.name}</strong><small>First seen {formatRelativeTime(m.firstSeen)}</small></span><span role="cell"><b className={m.loaded ? "loaded-label" : ""}>{runtime.status !== "healthy" ? "Stale inventory" : m.loaded === null ? "Not reported" : m.loaded ? "Loaded" : "Available"}</b><small>Seen {formatRelativeTime(m.lastSeen)}</small></span><span role="cell">{m.contextLength?.toLocaleString() ?? "—"}</span><span role="cell">{m.quantization ?? "—"}</span><span role="cell">{m.parameters ?? "—"}<small>{m.sizeBytes ? `${(m.sizeBytes / 1e9).toFixed(1)} GB` : ""}</small></span></div>)}</div> : <div className="inventory-empty"><Server /><span>{runtime.status === "healthy" ? "Server detected. No available models reported." : runtime.enabled ? "Start the runtime to discover its models." : "Discovery is disabled for this runtime."}</span></div>}
    {runtime.models.length > 4 ? <button type="button" className="text-button" onClick={() => setExpanded(!expanded)}>{expanded ? "Show fewer models" : `Show all ${runtime.models.length} models`}</button> : null}
  </article>;
}

export function LocalModelSettings({ data, onRefresh }: { data: LocalModelsSnapshot; onRefresh: () => Promise<void> }) {
  return <section className="settings-section"><div className="settings-heading"><h2>Local Models</h2><p>Automatic discovery on this computer. Override a port when your runtime uses a different endpoint.</p></div><div className="settings-panel local-settings">{data.runtimes.map((runtime) => <RuntimeSetting key={`${runtime.provider}:${runtime.endpoint}:${runtime.enabled}`} runtime={runtime} onRefresh={onRefresh} />)}</div><p className="local-qualifier">oMLX history: {data.historyStatus.replaceAll("_", " ")}. Reads the standard local usage database when present. No prompts, responses, raw logs, or credentials are collected.</p></section>;
}

function RuntimeSetting({ runtime, onRefresh }: { runtime: LocalRuntime; onRefresh: () => Promise<void> }) {
  const [enabled, setEnabled] = useState(runtime.enabled);
  const [automatic, setAutomatic] = useState(runtime.automatic);
  const [endpoint, setEndpoint] = useState(runtime.endpoint);
  const [busy, setBusy] = useState(false);
  const [message, setMessage] = useState<string | null>(null);
  async function perform(save: boolean) {
    setBusy(true); setMessage(null);
    try {
      if (save) { await configureLocalRuntime(runtime.provider, enabled, automatic ? null : endpoint); await onRefresh(); setMessage("Configuration saved"); }
      else setMessage(await testLocalRuntime(runtime.provider, automatic ? defaultEndpoint(runtime.provider) : endpoint));
    } catch (error) { setMessage(error instanceof Error ? error.message : String(error)); }
    finally { setBusy(false); }
  }
  return <div className="runtime-setting"><div className="runtime-heading"><ProviderMark provider={runtime.provider} /><strong>{providerLabel(runtime.provider)}</strong><RuntimeStatus runtime={runtime} /><label className="local-enable"><input aria-label={`Enable ${providerLabel(runtime.provider)}`} type="checkbox" checked={enabled} disabled={busy} onChange={(e) => setEnabled(e.target.checked)} />Enabled</label></div><div className="endpoint-controls"><label><span>Endpoint</span><input aria-label={`${providerLabel(runtime.provider)} endpoint`} type="url" value={automatic ? defaultEndpoint(runtime.provider) : endpoint} disabled={automatic || busy || !enabled} onChange={(e) => setEndpoint(e.target.value)} spellCheck={false} /></label><label className="automatic-control"><input type="checkbox" aria-label={`${providerLabel(runtime.provider)} automatic discovery`} checked={automatic} disabled={busy || !enabled} onChange={(e) => setAutomatic(e.target.checked)} />Automatic</label><button className="secondary-button" type="button" disabled={busy || !enabled} onClick={() => void perform(false)}>Test connection</button><button className="secondary-button" type="button" disabled={busy} onClick={() => void perform(true)}>{busy ? "Working…" : "Save"}</button></div><p className="runtime-diagnostic">{runtime.diagnostic}{runtime.detectedAt ? ` · Last successful detection ${formatRelativeTime(runtime.detectedAt)}` : ""}</p>{message ? <p role="status" className="connection-result">{message}</p> : null}</div>;
}

function defaultEndpoint(provider: string) { return provider === "omlx" ? "http://127.0.0.1:8000/v1" : provider === "ollama" ? "http://127.0.0.1:11434" : "http://127.0.0.1:1234"; }
