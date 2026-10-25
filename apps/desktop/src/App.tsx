import { errorPreview, loadingPreview, samplePreview } from "./lib/preview";
import { Activity as ActivityIcon, BarChart3, Cpu, Gauge, PanelsTopLeft, RefreshCw, Settings as SettingsIcon, ShieldCheck, Monitor, WifiOff } from "lucide-react";
import { isTauri } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { useCallback, useEffect, useRef, useState } from "react";
import { Activity } from "./components/Activity";
import { Insights } from "./components/Insights";
import { Overview } from "./components/Overview";
import { Sessions } from "./components/Sessions";
import { RangeSelector } from "./components/RangeSelector";
import { Settings } from "./components/Settings";
import { LocalModels } from "./components/LocalModels";
import { AppUpdaterProvider, UpdateBanner } from "./components/AppUpdater";
import { getActivityPage, getClaudeQuotaStatus, getDashboard, getGrokQuotaStatus, getSetting, refreshClaudeQuota, refreshGrokQuota, renameDevice, saveSubscription, scanNow, setClaudeQuotaEnabled, setGrokQuotaEnabled, syncCloudNow } from "./lib/api";
import { formatRelativeTime } from "./lib/format";
import type { DashboardSnapshot, NavKey, ProviderQuotaState, RangeKey, Subscription } from "./types";

const navigation: { key: NavKey; label: string; icon: typeof Gauge }[] = [
  { key: "overview", label: "Overview", icon: Gauge },
  { key: "activity", label: "Activity", icon: ActivityIcon },
  { key: "sessions", label: "Sessions", icon: PanelsTopLeft },
  { key: "local-models", label: "Local Models", icon: Cpu },
  { key: "insights", label: "Insights", icon: BarChart3 },
  { key: "settings", label: "Settings", icon: SettingsIcon },
];

export default function App() {
  const loadVersion = useRef(0);
  const [nav, setNav] = useState<NavKey>("overview");
  const [range, setRange] = useState<RangeKey>("today");
  const [data, setData] = useState<DashboardSnapshot | null>(null);
  const [claudeQuota, setClaudeQuota] = useState<ProviderQuotaState | null>(null);
  const [grokQuota, setGrokQuota] = useState<ProviderQuotaState | null>(null);
  const [loading, setLoading] = useState(true);
  const [scanning, setScanning] = useState(false);
  const [loadingActivity, setLoadingActivity] = useState(false);
  const [activityHasMore, setActivityHasMore] = useState(false);
  const [settingsSection, setSettingsSection] = useState("general");
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    void getSetting("theme").then((theme) => { document.documentElement.dataset.theme = theme ?? "system"; }).catch(() => { document.documentElement.dataset.theme = "system"; });
  }, []);

  const load = useCallback(async (nextRange: RangeKey = range) => {
    const version = ++loadVersion.current;
    try {
      const [snapshotResult, claudeQuotaResult, grokQuotaResult] = await Promise.allSettled([getDashboard(nextRange), getClaudeQuotaStatus(), getGrokQuotaStatus()]);
      if (version !== loadVersion.current) return;
      if (snapshotResult.status === "rejected") throw snapshotResult.reason;
      const snapshot = snapshotResult.value;
      setData(snapshot);
      if (claudeQuotaResult.status === "fulfilled") setClaudeQuota(claudeQuotaResult.value);
      if (grokQuotaResult.status === "fulfilled") setGrokQuota(grokQuotaResult.value);
      setActivityHasMore(snapshot.activity.length === 200);
      setError(null);
    } catch (reason) {
      if (version === loadVersion.current) setError(reason instanceof Error ? reason.message : String(reason));
    } finally {
      if (version === loadVersion.current) setLoading(false);
    }
  }, [range]);

  const scan = useCallback(async () => {
    if (scanning) return;
    setScanning(true);
    try {
      await scanNow();
      await load();
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : String(reason));
    } finally {
      setScanning(false);
    }
  }, [load, scanning]);

  useEffect(() => {
    const timer = window.setTimeout(() => { void load(); }, 0);
    return () => window.clearTimeout(timer);
  }, [load]);
  useEffect(() => {
    if (!isTauri()) return;
    let disposed = false;
    let unlisten: (() => void) | undefined;
    void listen<ProviderQuotaState>("arcmeter://quota-changed", (event) => {
      if (event.payload.provider === "grok") setGrokQuota(event.payload);
      else if (event.payload.provider === "claude") setClaudeQuota(event.payload);
    }).then((stop) => {
      if (disposed) stop(); else unlisten = stop;
    });
    return () => { disposed = true; unlisten?.(); };
  }, []);
  useEffect(() => {
    const timer = window.setTimeout(() => { void scan(); }, 450);
    return () => window.clearTimeout(timer);
    // Initial collection runs once; later scans are explicit or scheduled natively.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);
  useEffect(() => {
    if (!isTauri()) return;
    let disposed = false;
    let unlisten: (() => void) | undefined;
    void listen("arcmeter://data-changed", () => { void load(); }).then((stop) => {
      if (disposed) stop();
      else unlisten = stop;
    });
    return () => {
      disposed = true;
      unlisten?.();
    };
  }, [load]);

  function changeRange(next: RangeKey) {
    if (next === range) return;
    setLoading(true);
    setRange(next);
  }

  async function updateSubscription(subscription: Subscription) {
    await saveSubscription(subscription);
    await load();
  }

  async function updateDevice(name: string) {
    const device = await renameDevice(name);
    await load();
    return device;
  }

  async function syncNow() {
    await scan();
    await syncCloudNow();
    await load();
  }

  async function loadOlderActivity() {
    if (!data || loadingActivity) return;
    setLoadingActivity(true);
    try {
      const older = await getActivityPage(range, 200, data.activity.length);
      setData((current) => {
        if (!current) return current;
        const seen = new Set(current.activity.map((item) => item.id));
        return { ...current, activity: [...current.activity, ...older.filter((item) => !seen.has(item.id))] };
      });
      setActivityHasMore(older.length === 200);
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : String(reason));
    } finally {
      setLoadingActivity(false);
    }
  }

  const sourceWarning = data?.sources.some((source) => source.detected && source.status !== "healthy");
  const collectionLabel = loading && !data ? "Loading ledger" : scanning ? "Collecting" : error ? "Needs attention" : sourceWarning ? "Source warning" : data?.sources.some((source) => source.detected) ? "Local ledger current" : "No sources detected";
  const title = navigation.find((item) => item.key === nav)?.label ?? "ArcMeter";

  return (
    <AppUpdaterProvider os={data?.device.os ?? null}>
    <div className="app-shell">

      <a className="skip-link" href="#workspace">Skip to workspace</a>
      <aside className="app-sidebar">
        <button type="button" className="brand" onClick={() => setNav("overview")} aria-label="Open Overview">
          <span className="brand-mark"><i /><i /><i /></span><span>ArcMeter<small>AI usage monitor</small></span>
        </button>
        <nav aria-label="Main navigation">
          {["Monitor", "Analyze", "System"].map((group, index) => <div className="nav-group" key={group}>
            <p>{group}</p>
            {navigation.filter((item) => index === 0 ? ["overview", "activity", "sessions", "local-models"].includes(item.key) : index === 1 ? item.key === "insights" : item.key === "settings").map((item) => {
              const Icon = item.icon;
              return <button key={item.key} type="button" aria-current={nav === item.key ? "page" : undefined}
                aria-label={item.label} title={item.label} className={nav === item.key ? "active" : ""} onClick={() => setNav(item.key)}><Icon /><span>{item.label}</span></button>;
            })}
          </div>)}
        </nav>
        <div className="sidebar-footer">
          <div className="sidebar-device"><Monitor /><div><strong>{data?.device.friendlyName ?? "This computer"}</strong><small>v{data?.device.appVersion ?? "0.4.0"} · {isTauri() ? "Desktop" : "Browser preview"}</small></div></div>
          <span className="privacy-status"><ShieldCheck />Metadata only · private by default</span>
        </div>
      </aside>
      <div className="workspace">
        <header className="workspace-toolbar">
          <div className="workspace-location"><span>Workspace</span><span>/</span><strong>{title}</strong>{samplePreview || errorPreview || loadingPreview ? <span className="sample-preview-label">{samplePreview ? "Sample data" : errorPreview ? "Error state" : "Loading state"} · visual QA</span> : null}</div>
          <div className="collection-status">
            {data ? <span className="updated-label">Updated {formatRelativeTime(data.generatedAt)}</span> : null}
            <button type="button" className="sync-control" onClick={() => void scan()} disabled={scanning} title="Collect local usage now">
              <span className={error || sourceWarning ? "sync-indicator error" : scanning || (loading && !data) ? "sync-indicator working" : "sync-indicator"} />
              <span>{collectionLabel}</span>
              <RefreshCw className={scanning ? "spin" : ""} />
            </button>
          </div>
        </header>
        <main id="workspace" tabIndex={-1}>
        <div className="page-header">
          <div><h1>{title}</h1><p>{nav === "overview" ? "Your AI activity, measured." : nav === "activity" ? "Every recorded event. Every telemetry layer." : nav === "sessions" ? "Explore the work behind your usage." : nav === "local-models" ? "Runtime inventory and measured local usage." : nav === "insights" ? "Patterns from your measured telemetry." : "Make ArcMeter work for you."}</p></div>
          {nav !== "settings" && nav !== "sessions" && nav !== "local-models" ? <RangeSelector value={range} onChange={(value) => void changeRange(value)} /> : null}
        </div>

        {nav !== "settings" ? <UpdateBanner onOpenSettings={() => setNav("settings")} /> : null}
        {error ? <div className="error-banner" role="alert"><WifiOff /><div><strong>{data ? "Showing last loaded telemetry" : "Unable to load your ledger"}</strong><span>{error}</span></div><button type="button" onClick={() => void load()}>Retry</button></div> : null}

        {loading && !data ? <LoadingState /> : data ? (
          <div key={nav} className={loading ? "page-content refreshing" : "page-content"} aria-busy={loading}>
            {nav === "overview" ? <Overview data={data} quotas={[claudeQuota, grokQuota]} scanning={scanning} onScan={() => void scan()} onLocalModels={() => setNav("local-models")} onActivity={() => setNav("activity")} /> : null}
            {nav === "local-models" ? <LocalModels data={data.localModels} scanning={scanning} onScan={() => void scan()} onSettings={() => { setSettingsSection("local-models"); setNav("settings"); }} /> : null}
            {nav === "activity" ? <Activity items={data.activity} hasMore={activityHasMore} loadingMore={loadingActivity} onLoadMore={loadOlderActivity} /> : null}
            {nav === "sessions" ? <Sessions /> : null}
            {nav === "insights" ? <Insights insights={data.insights} byModel={data.byModel} byProject={data.byProject} quotas={[claudeQuota, grokQuota]} telemetry={data.telemetry} byProvider={data.byProvider} /> : null}
            {nav === "settings" ? <Settings initialSection={settingsSection} data={data} claudeQuota={claudeQuota} grokQuota={grokQuota} scanning={scanning} onScan={scan} onSync={syncNow} onSaveSubscription={updateSubscription} onRenameDevice={updateDevice} onToggleClaudeQuota={async (enabled) => setClaudeQuota(await setClaudeQuotaEnabled(enabled))} onRefreshClaudeQuota={async () => setClaudeQuota(await refreshClaudeQuota())} onToggleGrokQuota={async (enabled) => setGrokQuota(await setGrokQuotaEnabled(enabled))} onRefreshGrokQuota={async () => setGrokQuota(await refreshGrokQuota())} /> : null}
          </div>
        ) : null}
      </main>
      </div>
    </div>
    </AppUpdaterProvider>
  );
}

function LoadingState() {
  return (
    <div className="loading-state" aria-label="Loading ArcMeter">
      <div className="loading-line wide" /><div className="loading-line medium" />
      <div className="loading-metrics"><i /><i /><i /><i /></div>
      <div className="loading-panels"><i /><i /></div>
    </div>
  );
}
