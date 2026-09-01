export type RangeKey = "today" | "7d" | "30d" | "month" | "all";
export type NavKey = "overview" | "activity" | "insights" | "settings";

export interface HeadlineMetrics {
  measuredTokensToday: number;
  measuredTokensMonth: number;
  measuredTokensRange: number;
  measuredEventsRange: number;
  pricedTokensRange: number;
  pricedEventsRange: number;
  activityMinutesRange: number;
  monthlySubscriptionUsdCents: number;
  estimatedApiValueUsdMicros: number | null;
  pricingComplete: boolean;
}

export interface TrendPoint {
  date: string;
  label: string;
  tokens: number;
}

export interface BreakdownItem {
  key: string;
  label: string;
  tokens: number;
  percentage: number;
}

export interface ActivityItem {
  id: string;
  provider: string;
  source: string;
  occurredAt: string;
  model: string | null;
  projectName: string | null;
  totalTokens: number;
  inputTokens: number;
  cachedInputTokens: number;
  cacheWriteTokens: number;
  cacheWrite5mTokens: number;
  cacheWrite1hTokens: number;
  outputTokens: number;
  reasoningTokens: number;
  nativeCostUsdTicks: number | null;
  estimatedApiValueUsdMicros: number | null;
  measurementKind: "measured" | "estimated" | "activity_only";
  deviceId: string;
  deviceName: string;
}

export interface Insight {
  id: string;
  title: string;
  detail: string;
  tone: string;
}

export interface CollectorDiagnostic {
  severity: "warning" | "error";
  code: string;
  message: string;
  recordNumber: number | null;
}

export interface SourceScanResult {
  provider: string;
  label: string;
  detected: boolean;
  filesSeen: number;
  recordsSeen: number;
  recordsInserted: number;
  measuredRecords: number;
  measuredSessions: number;
  measuredTurns: number;
  measuredTokens: number;
  cacheReadTokens: number;
  cacheWriteTokens: number;
  nativeCostUsdTicks: number | null;
  lastScanAt: string;
  lastUsageAt: string | null;
  status: "healthy" | "warning" | "error";
  diagnostics: CollectorDiagnostic[];
}

export interface Subscription {
  id: string;
  provider: string;
  planName: string;
  monthlyPriceUsdCents: number;
  billingCadence: "monthly" | "annual";
  active: boolean;
  updatedAt: string;
}

export interface Device {
  id: string;
  friendlyName: string;
  os: string;
  architecture: string;
  appVersion: string;
  createdAt: string;
  lastSeenAt: string;
  lastSyncAt: string | null;
  syncStatus: "local_only" | "synced" | "syncing" | "error";
}

export interface DashboardSnapshot {
  generatedAt: string;
  range: RangeKey;
  metrics: HeadlineMetrics;
  trend: TrendPoint[];
  byProvider: BreakdownItem[];
  byModel: BreakdownItem[];
  byProject: BreakdownItem[];
  byDevice: BreakdownItem[];
  activity: ActivityItem[];
  insights: Insight[];
  sources: SourceScanResult[];
  subscriptions: Subscription[];
  device: Device;
}

export interface ScanReport {
  sources: SourceScanResult[];
  totalInserted: number;
}

export interface AuthStatus {
  configured: boolean;
  signedIn: boolean;
  email: string | null;
  expiresAt: string | null;
}

export interface SyncReport {
  uploadedEvents: number;
  downloadedEvents: number;
  downloadedDevices: number;
  syncedSubscriptions: number;
  uploadedQuotaSnapshots: number;
  downloadedQuotaSnapshots: number;
  completedAt: string;
}

export type QuotaHealth =
  | "not_configured"
  | "credential_unavailable"
  | "permission_denied"
  | "expired_login"
  | "forbidden"
  | "rate_limited"
  | "provider_unavailable"
  | "offline"
  | "invalid_response"
  | "healthy";

export interface ProviderQuotaWindow {
  key: string;
  label: string;
  kind: "rolling" | "weekly" | "model_weekly" | "other";
  scope: string | null;
  utilizationBps: number;
  resetsAt: string | null;
}

export interface ExtraUsage {
  enabled: boolean;
  monthlyLimitMinor: number | null;
  usedCreditsMinor: number | null;
  utilizationBps: number | null;
  currency: string | null;
}

export interface ProviderQuotaState {
  provider: string;
  enabled: boolean;
  status: QuotaHealth;
  message: string;
  stale: boolean;
  windows: ProviderQuotaWindow[];
  extraUsage: ExtraUsage | null;
  observedAt: string | null;
  attemptedAt: string | null;
  retryAt: string | null;
  sourceDeviceId: string | null;
  sourceDeviceName: string | null;
}

export interface ActivityTrackingStatus {
  claudeDesktopSupported: boolean;
  claudeDesktopEnabled: boolean;
  claudeDesktopMinutes: number;
  claudeDesktopLastActivityAt: string | null;
  browserBridgeEnabled: boolean;
  browserBridgePort: number;
  pairingToken: string;
}
