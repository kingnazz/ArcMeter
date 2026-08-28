export type RangeKey = "today" | "7d" | "30d" | "month" | "all";
export type NavKey = "overview" | "activity" | "insights" | "settings";

export interface HeadlineMetrics {
  measuredTokensToday: number;
  measuredTokensMonth: number;
  measuredTokensRange: number;
  measuredEventsRange: number;
  monthlySubscriptionUsdCents: number;
  estimatedApiValueUsdMicros: number | null;
  pricingComplete: boolean;
  valueMultiple: number | null;
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
  outputTokens: number;
  reasoningTokens: number;
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
  measuredTokens: number;
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
  completedAt: string;
}
