import { invoke, isTauri } from "@tauri-apps/api/core";
import type { ActivityTrackingStatus, AuthStatus, DashboardSnapshot, Device, ProviderQuotaState, RangeKey, ScanReport, SessionDetail, SessionPage, SessionQuery, Subscription, SyncReport } from "../types";

const previewNow = new Date().toISOString();
const unavailableDevice: Device = {
  id: "browser-preview",
  friendlyName: "Local preview",
  os: "unknown",
  architecture: "unknown",
  appVersion: "0.2.2",
  createdAt: previewNow,
  lastSeenAt: previewNow,
  lastSyncAt: null,
  syncStatus: "local_only",
};

function emptySnapshot(range: RangeKey): DashboardSnapshot {
  return {
    generatedAt: new Date().toISOString(),
    range,
    metrics: {
      measuredTokensToday: 0,
      measuredTokensMonth: 0,
      measuredTokensRange: 0,
      measuredEventsRange: 0,
      pricedTokensRange: 0,
      pricedEventsRange: 0,
      activityMinutesRange: 0,
      monthlySubscriptionUsdCents: 0,
      estimatedApiValueUsdMicros: null,
      pricingComplete: false,
    },
    trend: [],
    byProvider: [],
    byModel: [],
    byProject: [],
    byDevice: [],
    activity: [],
    insights: [],
    sources: ["Codex", "Claude Code CLI", "Grok Build", "Gemini CLI"].map((label) => ({
      provider: label.split(" ")[0]?.toLowerCase() ?? label.toLowerCase(),
      label,
      detected: false,
      filesSeen: 0,
      recordsSeen: 0,
      recordsInserted: 0,
      measuredRecords: 0,
      measuredSessions: 0,
      measuredTurns: 0,
      measuredTokens: 0,
      cacheReadTokens: 0,
      cacheWriteTokens: 0,
      nativeCostUsdTicks: null,
      lastScanAt: new Date().toISOString(),
      lastUsageAt: null,
      status: "healthy" as const,
      diagnostics: [],
    })),
    subscriptions: [],
    device: unavailableDevice,
  };
}

export async function getDashboard(range: RangeKey): Promise<DashboardSnapshot> {
  if (!isTauri()) return emptySnapshot(range);
  return invoke<DashboardSnapshot>("dashboard_snapshot", { range });
}

export async function scanNow(): Promise<ScanReport> {
  if (!isTauri()) return { sources: emptySnapshot("month").sources, totalInserted: 0 };
  return invoke<ScanReport>("scan_now");
}

export async function getActivityPage(range: RangeKey, limit: number, offset: number): Promise<DashboardSnapshot["activity"]> {
  if (!isTauri()) return [];
  return invoke<DashboardSnapshot["activity"]>("activity_page", { range, limit, offset });
}

export async function saveSubscription(subscription: Subscription): Promise<Subscription[]> {
  if (!isTauri()) return [subscription];
  return invoke<Subscription[]>("save_subscription", { subscription });
}

export async function renameDevice(name: string): Promise<Device> {
  if (!isTauri()) return { ...unavailableDevice, friendlyName: name };
  return invoke<Device>("rename_device", { name });
}

export async function getSetting(key: string): Promise<string | null> {
  if (!isTauri()) return null;
  return invoke<string | null>("get_setting", { key });
}

export async function setSetting(key: string, value: string): Promise<void> {
  if (!isTauri()) return;
  return invoke("set_setting", { key, value });
}

export async function getActivityTrackingStatus(): Promise<ActivityTrackingStatus> {
  if (!isTauri()) return {
    claudeDesktopSupported: false,
    claudeDesktopEnabled: false,
    claudeDesktopMinutes: 0,
    claudeDesktopLastActivityAt: null,
    browserBridgeEnabled: false,
    browserBridgePort: 47_639,
    pairingToken: "",
  };
  return invoke<ActivityTrackingStatus>("activity_tracking_status");
}

export async function getAuthStatus(): Promise<AuthStatus> {
  if (!isTauri()) return { configured: false, signedIn: false, email: null, expiresAt: null };
  return invoke<AuthStatus>("auth_status");
}

export async function signIn(email: string, password: string): Promise<AuthStatus> {
  return invoke<AuthStatus>("auth_sign_in", { email, password });
}

export async function signOut(): Promise<AuthStatus> {
  return invoke<AuthStatus>("auth_sign_out");
}

export async function syncCloudNow(): Promise<SyncReport> {
  return invoke<SyncReport>("sync_now");
}

export async function getSessionPage(query: SessionQuery): Promise<SessionPage> {
  if (!isTauri()) return { sessions: [], totalCount: 0, stats: { sessionCount: 0, totalTokens: 0, estimatedApiValueUsdMicros: null }, hasMore: false };
  return invoke<SessionPage>("session_page", { query });
}

export async function getSessionDetail(session: Pick<SessionDetail["session"], "provider" | "source" | "nativeSessionId">, limit = 100, offset = 0): Promise<SessionDetail> {
  return invoke<SessionDetail>("session_detail", { provider: session.provider, source: session.source, nativeSessionId: session.nativeSessionId, limit, offset });
}

const previewQuota: ProviderQuotaState = {
  provider: "claude",
  enabled: false,
  status: "not_configured",
  message: "Claude live limits are off.",
  stale: false,
  windows: [],
  analyses: [],
  extraUsage: null,
  planLabel: null,
  source: "claude_code",
  observedAt: null,
  attemptedAt: null,
  retryAt: null,
  sourceDeviceId: null,
  sourceDeviceName: null,
};

export async function getClaudeQuotaStatus(): Promise<ProviderQuotaState> {
  if (!isTauri()) return previewQuota;
  return invoke<ProviderQuotaState>("claude_quota_status");
}

export async function setClaudeQuotaEnabled(enabled: boolean): Promise<ProviderQuotaState> {
  if (!isTauri()) return { ...previewQuota, enabled, status: enabled ? "credential_unavailable" : "not_configured" };
  return invoke<ProviderQuotaState>("set_claude_quota_enabled", { enabled });
}

export async function refreshClaudeQuota(): Promise<ProviderQuotaState> {
  if (!isTauri()) return previewQuota;
  return invoke<ProviderQuotaState>("refresh_claude_quota");
}

const previewGrokQuota: ProviderQuotaState = {
  ...previewQuota,
  provider: "grok",
  message: "Grok live limits are off.",
  source: "grok_cli",
};

export async function getGrokQuotaStatus(): Promise<ProviderQuotaState> {
  if (!isTauri()) return previewGrokQuota;
  return invoke<ProviderQuotaState>("grok_quota_status");
}

export async function setGrokQuotaEnabled(enabled: boolean): Promise<ProviderQuotaState> {
  if (!isTauri()) return { ...previewGrokQuota, enabled, status: enabled ? "credential_unavailable" : "not_configured" };
  return invoke<ProviderQuotaState>("set_grok_quota_enabled", { enabled });
}

export async function refreshGrokQuota(): Promise<ProviderQuotaState> {
  if (!isTauri()) return previewGrokQuota;
  return invoke<ProviderQuotaState>("refresh_grok_quota");
}
