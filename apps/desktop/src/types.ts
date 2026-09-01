export type RangeKey = "today" | "7d" | "30d" | "month" | "all";
export type NavKey = "overview" | "activity" | "sessions" | "insights" | "settings";

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

export type SessionPricingCoverage = "complete" | "partial" | "unavailable";

export interface SessionSummary {
  sessionKey: string;
  provider: string;
  source: string;
  nativeSessionId: string;
  projectName: string;
  startedAt: string;
  lastActivityAt: string;
  durationSeconds: number;
  eventCount: number;
  inputTokens: number;
  cachedInputTokens: number;
  cacheWriteTokens: number;
  cacheWrite5mTokens: number;
  cacheWrite1hTokens: number;
  outputTokens: number;
  reasoningTokens: number;
  totalTokens: number;
  estimatedApiValueUsdMicros: number | null;
  nativeCostUsdTicks: number | null;
  pricingCoverage: SessionPricingCoverage;
  primaryModel: string;
  modelCount: number;
  deviceCount: number;
  primaryDeviceName: string;
}

export interface SessionStats {
  sessionCount: number;
  totalTokens: number;
  estimatedApiValueUsdMicros: number | null;
}

export interface SessionPage {
  sessions: SessionSummary[];
  totalCount: number;
  stats: SessionStats;
  hasMore: boolean;
}

export interface SessionModel {
  model: string;
  tokens: number;
  eventCount: number;
}

export interface SessionTimelineItem {
  occurredAt: string;
  model: string;
  totalTokens: number;
  estimatedApiValueUsdMicros: number | null;
}

export interface SessionDetail {
  session: SessionSummary;
  models: SessionModel[];
  devices: string[];
  events: SessionTimelineItem[];
  eventsHasMore: boolean;
}

export interface SessionQuery {
  range: "today" | "7d" | "30d" | "all";
  provider?: string;
  search?: string;
  sort?: "recent" | "tokens" | "value" | "duration";
  limit?: number;
  offset?: number;
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
  kind: "rolling" | "weekly" | "monthly" | "model_weekly" | "product" | "other";
  scope: string | null;
  utilizationBps: number;
  periodStartsAt: string | null;
  resetsAt: string | null;
}

export type QuotaAnalysisConfidence = "insufficient" | "low" | "medium" | "high";
export type QuotaAnalysisStatus = "gathering" | "no_recent_change" | "active" | "limit_reached" | "informational" | "stale";

export interface QuotaWindowAnalysis {
  provider: string;
  windowKey: string;
  label: string;
  kind: ProviderQuotaWindow["kind"];
  capBearing: boolean;
  utilizationBps: number;
  remainingBps: number;
  periodStartsAt: string | null;
  resetsAt: string | null;
  observedAt: string;
  recentBurnBpsPerHour: number | null;
  periodAverageBurnBpsPerHour: number | null;
  projectedExhaustionAt: string | null;
  projectedBeforeReset: boolean | null;
  sampleCount: number;
  observationSpanSeconds: number;
  confidence: QuotaAnalysisConfidence;
  status: QuotaAnalysisStatus;
  stale: boolean;
}

export interface ExtraUsage {
  enabled: boolean;
  monthlyLimitMinor: number | null;
  usedCreditsMinor: number | null;
  prepaidBalanceMinor: number | null;
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
  analyses: QuotaWindowAnalysis[];
  extraUsage: ExtraUsage | null;
  planLabel: string | null;
  source: "claude_code" | "grok_cli" | "grok_web" | "cloud_sync";
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
