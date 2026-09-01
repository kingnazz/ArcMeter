import { Activity as ActivityIcon, BarChart3, Gauge, RefreshCw, Settings as SettingsIcon, WifiOff } from "lucide-react";
import { isTauri } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { useCallback, useEffect, useState } from "react";
import { Activity } from "./components/Activity";
import { Insights } from "./components/Insights";
import { Overview } from "./components/Overview";
import { RangeSelector } from "./components/RangeSelector";
import { Settings } from "./components/Settings";
import { AppUpdaterProvider, UpdateBanner } from "./components/AppUpdater";
import { getActivityPage, getClaudeQuotaStatus, getDashboard, refreshClaudeQuota, renameDevice, saveSubscription, scanNow, setClaudeQuotaEnabled, syncCloudNow } from "./lib/api";
import { formatRelativeTime } from "./lib/format";
import type { DashboardSnapshot, NavKey, ProviderQuotaState, RangeKey, Subscription } from "./types";

const navigation: { key: NavKey; label: string; icon: typeof Gauge }[] = [
  { key: "overview", label: "Overview", icon: Gauge },
  { key: "activity", label: "Activity", icon: ActivityIcon },
  { key: "insights", label: "Insights", icon: BarChart3 },
  { key: "settings", label: "Settings", icon: SettingsIcon },
];

export default function App() {
  const [nav, setNav] = useState<NavKey>("overview");
  const [range, setRange] = useState<RangeKey>("month");
  const [data, setData] = useState<DashboardSnapshot | null>(null);
  const [claudeQuota, setClaudeQuota] = useState<ProviderQuotaState | null>(null);
  const [loading, setLoading] = useState(true);
  const [scanning, setScanning] = useState(false);
  const [loadingActivity, setLoadingActivity] = useState(false);
  const [activityHasMore, setActivityHasMore] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const load = useCallback(async (nextRange: RangeKey = range) => {
    try {
      const [snapshotResult, quotaResult] = await Promise.allSettled([getDashboard(nextRange), getClaudeQuotaStatus()]);
      if (snapshotResult.status === "rejected") throw snapshotResult.reason;
      const snapshot = snapshotResult.value;
      setData(snapshot);
      if (quotaResult.status === "fulfilled") setClaudeQuota(quotaResult.value);
      setActivityHasMore(snapshot.activity.length === 200);
      setError(null);
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : String(reason));
    } finally {
      setLoading(false);
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
    void listen<ProviderQuotaState>("arcmeter://quota-changed", (event) => setClaudeQuota(event.payload)).then((stop) => {
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

  async function changeRange(next: RangeKey) {
    setRange(next);
    setLoading(true);
    await load(next);
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

  const title = navigation.find((item) => item.key === nav)?.label ?? "ArcMeter";

  return (
    <AppUpdaterProvider os={data?.device.os ?? null}>
    <div className="app-shell">
      <header className="app-header">
        <button type="button" className="brand" onClick={() => setNav("overview")} aria-label="Open Overview">
          <span className="brand-mark"><i /><i /><i /></span>
          <span>ArcMeter</span>
          <small>V1</small>
        </button>
        <nav aria-label="Main navigation">
          {navigation.map((item) => {
            const Icon = item.icon;
            return <button key={item.key} type="button" className={nav === item.key ? "active" : ""} onClick={() => setNav(item.key)}><Icon />{item.label}</button>;
          })}
        </nav>
        <button type="button" className="sync-control" onClick={() => void scan()} disabled={scanning} title="Collect local usage now">
          <span className={error ? "sync-indicator error" : scanning ? "sync-indicator working" : "sync-indicator"} />
          <span>{scanning ? "Collecting" : error ? "Needs attention" : "Local ledger current"}</span>
          <RefreshCw className={scanning ? "spin" : ""} />
        </button>
      </header>

      <main>
        <div className="page-header">
          <div>
            <p className="eyebrow">{nav === "overview" ? "Usage intelligence" : nav === "activity" ? "Measured event ledger" : nav === "insights" ? "Reliable comparisons" : "ArcMeter preferences"}</p>
            <h1>{title}</h1>
          </div>
          {nav !== "settings" ? <RangeSelector value={range} onChange={(value) => void changeRange(value)} /> : data ? <span className="updated-label">Updated {formatRelativeTime(data.generatedAt)}</span> : null}
        </div>

        {nav !== "settings" ? <UpdateBanner onOpenSettings={() => setNav("settings")} /> : null}
        {error ? <div className="error-banner" role="alert"><WifiOff /><div><strong>ArcMeter is still available offline</strong><span>{error}</span></div><button type="button" onClick={() => void load()}>Retry</button></div> : null}

        {loading && !data ? <LoadingState /> : data ? (
          <div className={loading ? "page-content refreshing" : "page-content"}>
            {nav === "overview" ? <Overview data={data} claudeQuota={claudeQuota} scanning={scanning} onScan={() => void scan()} /> : null}
            {nav === "activity" ? <Activity items={data.activity} hasMore={activityHasMore} loadingMore={loadingActivity} onLoadMore={loadOlderActivity} /> : null}
            {nav === "insights" ? <Insights insights={data.insights} byModel={data.byModel} byProject={data.byProject} /> : null}
            {nav === "settings" ? <Settings data={data} claudeQuota={claudeQuota} scanning={scanning} onScan={scan} onSync={syncNow} onSaveSubscription={updateSubscription} onRenameDevice={updateDevice} onToggleClaudeQuota={async (enabled) => setClaudeQuota(await setClaudeQuotaEnabled(enabled))} onRefreshClaudeQuota={async () => setClaudeQuota(await refreshClaudeQuota())} /> : null}
          </div>
        ) : null}
      </main>
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
