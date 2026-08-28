import { invoke, isTauri } from "@tauri-apps/api/core";
import type { AuthStatus, DashboardSnapshot, Device, RangeKey, ScanReport, Subscription, SyncReport } from "../types";

const previewNow = new Date().toISOString();
const unavailableDevice: Device = {
  id: "browser-preview",
  friendlyName: "Local preview",
  os: "unknown",
  architecture: "unknown",
  appVersion: "0.1.0",
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
      monthlySubscriptionUsdCents: 0,
      estimatedApiValueUsdMicros: null,
      pricingComplete: false,
      valueMultiple: null,
    },
    trend: [],
    byProvider: [],
    byModel: [],
    byProject: [],
    byDevice: [],
    activity: [],
    insights: [],
    sources: ["Codex", "Claude Code", "Grok Build", "Gemini CLI"].map((label) => ({
      provider: label.split(" ")[0]?.toLowerCase() ?? label.toLowerCase(),
      label,
      detected: false,
      filesSeen: 0,
      recordsSeen: 0,
      recordsInserted: 0,
      measuredRecords: 0,
      measuredTokens: 0,
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
