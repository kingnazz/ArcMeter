import { cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import type { CacheEfficiencyReport, DashboardSnapshot, ProviderQuotaState, SessionDetail, SessionPage } from "../types";
import { Activity } from "./Activity";
import { Insights } from "./Insights";
import { Overview } from "./Overview";
import { Settings } from "./Settings";
import { Sessions } from "./Sessions";

const now = "2026-08-28T07:00:00Z";

afterEach(() => {
  cleanup();
  vi.useRealTimers();
});

function snapshot(): DashboardSnapshot {
  return {
    generatedAt: now,
    range: "month",
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
    sources: [
      {
        provider: "codex",
        label: "Codex",
        detected: true,
        filesSeen: 2,
        recordsSeen: 0,
        recordsInserted: 0,
        measuredRecords: 0,
        measuredSessions: 0,
        measuredTurns: 0,
        measuredTokens: 0,
        cacheReadTokens: 0,
        cacheWriteTokens: 0,
        nativeCostUsdTicks: null,
        lastScanAt: now,
        lastUsageAt: null,
        status: "healthy",
        diagnostics: [],
      },
      {
        provider: "claude",
        label: "Claude Code CLI",
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
        lastScanAt: now,
        lastUsageAt: null,
        status: "healthy",
        diagnostics: [],
      },
    ],
    subscriptions: [],
    device: {
      id: "device-1",
      friendlyName: "Windows Business Desktop",
      os: "windows",
      architecture: "x86_64",
      appVersion: "0.2.0",
      createdAt: now,
      lastSeenAt: now,
      lastSyncAt: null,
      syncStatus: "local_only",
    },
  };
}

function quota(overrides: Partial<ProviderQuotaState> = {}): ProviderQuotaState {
  return {
    provider: "claude",
    enabled: true,
    status: "healthy",
    message: "Connected through Claude Code.",
    stale: false,
    windows: [
      { key: "five_hour", label: "5-hour", kind: "rolling", scope: null, utilizationBps: 4_763, periodStartsAt: null, resetsAt: "2026-08-31T14:14:00Z" },
      { key: "seven_day", label: "Weekly", kind: "weekly", scope: null, utilizationBps: 1_700, periodStartsAt: null, resetsAt: "2026-09-04T02:00:00Z" },
      { key: "weekly_fable", label: "Fable", kind: "model_weekly", scope: "fable", utilizationBps: 800, periodStartsAt: null, resetsAt: null },
    ],
    analyses: [],
    extraUsage: { enabled: true, monthlyLimitMinor: 5_000, usedCreditsMinor: 1_242, prepaidBalanceMinor: null, utilizationBps: 2_484, currency: "USD" },
    planLabel: null,
    source: "claude_code",
    observedAt: "2026-08-31T12:00:00Z",
    attemptedAt: "2026-08-31T12:00:00Z",
    retryAt: null,
    sourceDeviceId: "device-1",
    sourceDeviceName: "MacBook",
    ...overrides,
  };
}

function analysis(overrides: Partial<ProviderQuotaState["analyses"][number]> = {}): ProviderQuotaState["analyses"][number] {
  return {
    provider: "claude",
    windowKey: "five_hour",
    label: "5-hour",
    kind: "rolling",
    capBearing: true,
    utilizationBps: 6_420,
    remainingBps: 3_580,
    periodStartsAt: null,
    resetsAt: "2026-08-31T18:00:00Z",
    observedAt: "2026-08-31T12:00:00Z",
    recentBurnBpsPerHour: 840,
    periodAverageBurnBpsPerHour: null,
    projectedExhaustionAt: "2026-08-31T16:15:00Z",
    projectedBeforeReset: true,
    sampleCount: 7,
    observationSpanSeconds: 3_600,
    confidence: "high",
    status: "active",
    stale: false,
    ...overrides,
  };
}

function sessionPage(overrides: Partial<SessionPage> = {}): SessionPage {
  return {
    sessions: [
      {
        sessionKey: "session-claude",
        provider: "claude",
        source: "claude_code",
        nativeSessionId: "do-not-display-claude-session",
        projectName: "ArcMeter",
        startedAt: "2026-09-01T10:00:00Z",
        lastActivityAt: "2026-09-01T10:42:00Z",
        durationSeconds: 2_520,
        eventCount: 3,
        inputTokens: 500,
        cachedInputTokens: 300,
        cacheWriteTokens: 140,
        cacheWrite5mTokens: 90,
        cacheWrite1hTokens: 50,
        outputTokens: 220,
        reasoningTokens: 80,
        totalTokens: 1_160,
        estimatedApiValueUsdMicros: 12_340,
        nativeCostUsdTicks: 120_000_000,
        pricingCoverage: "partial",
        primaryModel: "claude-sonnet",
        modelCount: 2,
        deviceCount: 2,
        primaryDeviceName: "MacBook",
      },
      {
        sessionKey: "session-grok",
        provider: "grok",
        source: "grok_build",
        nativeSessionId: "do-not-display-grok-session",
        projectName: "Pine",
        startedAt: "2026-08-20T10:00:00Z",
        lastActivityAt: "2026-08-20T10:10:00Z",
        durationSeconds: 600,
        eventCount: 1,
        inputTokens: 80,
        cachedInputTokens: 40,
        cacheWriteTokens: 0,
        cacheWrite5mTokens: 0,
        cacheWrite1hTokens: 0,
        outputTokens: 20,
        reasoningTokens: 5,
        totalTokens: 100,
        estimatedApiValueUsdMicros: null,
        nativeCostUsdTicks: null,
        pricingCoverage: "unavailable",
        primaryModel: "grok-build",
        modelCount: 1,
        deviceCount: 1,
        primaryDeviceName: "Windows Desktop",
      },
    ],
    totalCount: 2,
    stats: { sessionCount: 2, totalTokens: 1_260, estimatedApiValueUsdMicros: 12_340 },
    hasMore: false,
    ...overrides,
  };
}

function sessionDetail(page = sessionPage()): SessionDetail {
  const session = page.sessions[0]!;
  return {
    session,
    cache: {
      semanticCoverage: "complete",
      freshInputTokens: 500,
      cachedInputTokens: 300,
      cacheWriteTokens: 140,
      cacheWrite5mTokens: 90,
      cacheWrite1hTokens: 50,
      cacheWriteUnspecifiedTokens: 0,
      normalizedInputContextTokens: 940,
      reuseShareBps: 3_191,
      apiEquivalentCacheImpactUsdMicros: 1_500_000,
      cachePricingCoverage: "complete",
      measuredEventCount: 3,
    },
    models: [{ model: "claude-sonnet", tokens: 900, eventCount: 2 }, { model: "claude-opus", tokens: 260, eventCount: 1 }],
    devices: ["MacBook", "Windows Desktop"],
    events: [{ occurredAt: "2026-09-01T10:00:00Z", model: "claude-sonnet", totalTokens: 600, estimatedApiValueUsdMicros: 6_000 }],
    eventsHasMore: false,
  };
}

function cacheReport(overrides: Partial<CacheEfficiencyReport["summary"]> = {}): CacheEfficiencyReport {
  const summary = {
    semanticCoverage: "complete" as const,
    freshInputTokens: 4_800_000,
    cachedInputTokens: 18_400_000,
    cacheWriteTokens: 1_600_000,
    cacheWrite5mTokens: 840_000,
    cacheWrite1hTokens: 210_000,
    cacheWriteUnspecifiedTokens: 550_000,
    normalizedInputContextTokens: 24_800_000,
    reuseShareBps: 7_419,
    apiEquivalentCacheImpactUsdMicros: 21_840_000,
    cachePricingCoverage: "partial" as const,
    measuredEventCount: 42,
    ...overrides,
  };
  const row = { key: "claude:claude_code", label: "Claude Code", provider: "claude", source: "claude_code", model: null, project: null, ...summary };
  return {
    range: "7d",
    providerFilter: null,
    availableProviders: ["claude", "codex"],
    summary,
    byProvider: [row],
    byModel: [{ ...row, key: "claude:sonnet", label: "Claude Sonnet 5", source: null, model: "Claude Sonnet 5" }],
    byProject: [{ ...row, key: "ArcMeter", label: "ArcMeter", provider: null, source: null, project: "ArcMeter" }],
  };
}

describe("important product states", () => {
  it("renders cache reuse, canonical counters, TTL detail, partial coverage, and breakdowns", () => {
    render(<Insights insights={[]} byModel={[]} byProject={[]} initialCache={cacheReport()} />);
    expect(screen.getByRole("heading", { name: "Measured input reuse" })).toBeInTheDocument();
    expect(screen.getAllByText("74.2%").length).toBeGreaterThan(0);
    expect(screen.getAllByText("18.4M").length).toBeGreaterThan(0);
    expect(screen.getByText("4.8M")).toBeInTheDocument();
    expect(screen.getByText("5-minute")).toBeInTheDocument();
    expect(screen.getByText("1-hour")).toBeInTheDocument();
    expect(screen.getByText("Unspecified")).toBeInTheDocument();
    expect(screen.getByText(/Partial pricing coverage/)).toBeInTheDocument();
    expect(screen.getByText("Claude Sonnet 5")).toBeInTheDocument();
    expect(screen.getByText("ArcMeter")).toBeInTheDocument();
  });

  it("shows partial semantics and negative cache creation impact without calling tokens saved", () => {
    render(<Insights insights={[]} byModel={[]} byProject={[]} initialCache={cacheReport({ semanticCoverage: "partial", apiEquivalentCacheImpactUsdMicros: -420_000, cachePricingCoverage: "complete" })} />);
    expect(screen.getByText(/Partial semantic coverage/)).toBeInTheDocument();
    expect(screen.getByText(/higher/)).toBeInTheDocument();
    expect(screen.getByText(/Cache creation exceeded reuse/)).toBeInTheDocument();
    expect(document.body.textContent).not.toMatch(/tokens saved/i);
  });

  it("distinguishes no measured cache events from measured events without cache telemetry", () => {
    const empty = cacheReport({ measuredEventCount: 0, cachedInputTokens: 0, cacheWriteTokens: 0 });
    const view = render(<Insights insights={[]} byModel={[]} byProject={[]} initialCache={empty} />);
    expect(screen.getByText("No measured cache activity yet.")).toBeInTheDocument();
    view.unmount();
    render(<Insights insights={[]} byModel={[]} byProject={[]} initialCache={cacheReport({ measuredEventCount: 4, cachedInputTokens: 0, cacheWriteTokens: 0 })} />);
    expect(screen.getByText("Cache telemetry is unavailable for the providers in this range.")).toBeInTheDocument();
  });

  it("keeps every range provider available while switching directly between filtered analytics", async () => {
    const initial = { ...cacheReport(), availableProviders: ["grok", "codex", "claude", "codex"] };
    const loadCache = vi.fn((range: CacheEfficiencyReport["range"], provider?: string) => Promise.resolve({
      ...cacheReport({ cachedInputTokens: provider === "claude" ? 222 : 333 }),
      range,
      providerFilter: provider ?? null,
      availableProviders: ["grok", "codex", "claude"],
    }));
    render(<Insights insights={[]} byModel={[]} byProject={[]} initialCache={initial} loadCache={loadCache} />);

    const providerSelect = screen.getByLabelText("Cache provider");
    expect(within(providerSelect).getAllByRole("option").map((option) => option.textContent)).toEqual([
      "All providers",
      "Claude Code CLI",
      "Codex",
      "Grok Build",
    ]);

    fireEvent.change(providerSelect, { target: { value: "claude" } });
    await waitFor(() => expect(loadCache).toHaveBeenNthCalledWith(1, "7d", "claude"));
    expect(providerSelect).toHaveValue("claude");
    expect(within(providerSelect).getAllByRole("option").map((option) => option.textContent)).toEqual([
      "All providers",
      "Claude Code CLI",
      "Codex",
      "Grok Build",
    ]);
    expect(screen.getAllByText("222").length).toBeGreaterThan(0);

    fireEvent.change(providerSelect, { target: { value: "grok" } });
    await waitFor(() => expect(loadCache).toHaveBeenNthCalledWith(2, "7d", "grok"));
    expect(providerSelect).toHaveValue("grok");
    expect(loadCache).toHaveBeenCalledTimes(2);
  });

  it("preserves a selected provider when a new range has no matching rows", async () => {
    const initial = { ...cacheReport(), providerFilter: "claude", availableProviders: ["claude", "codex", "grok"] };
    const loadCache = vi.fn((range: CacheEfficiencyReport["range"], provider?: string) => Promise.resolve({
      ...cacheReport({ measuredEventCount: 0, cachedInputTokens: 0, cacheWriteTokens: 0 }),
      range,
      providerFilter: provider ?? null,
      availableProviders: ["grok", "codex"],
      byProvider: [],
      byModel: [],
      byProject: [],
    }));
    render(<Insights insights={[]} byModel={[]} byProject={[]} initialCache={initial} loadCache={loadCache} />);

    fireEvent.change(screen.getByLabelText("Cache date range"), { target: { value: "30d" } });
    await waitFor(() => expect(loadCache).toHaveBeenNthCalledWith(1, "30d", "claude"));
    const providerSelect = screen.getByLabelText("Cache provider");
    expect(providerSelect).toHaveValue("claude");
    expect(within(providerSelect).getAllByRole("option").map((option) => option.textContent)).toEqual([
      "All providers",
      "Claude Code CLI",
      "Codex",
      "Grok Build",
    ]);
    expect(screen.getByText("No measured cache activity yet.")).toBeInTheDocument();

    fireEvent.change(providerSelect, { target: { value: "codex" } });
    await waitFor(() => expect(loadCache).toHaveBeenNthCalledWith(2, "30d", "codex"));
    expect(providerSelect).toHaveValue("codex");
    expect(loadCache).toHaveBeenCalledTimes(2);
  });

  it("shows gathering, active, reset-first, flat, stale, and reached quota pace states", () => {
    vi.useFakeTimers();
    vi.setSystemTime(new Date("2026-08-31T12:00:00Z"));
    const cases = [
      analysis({ status: "gathering", confidence: "insufficient", recentBurnBpsPerHour: null, projectedExhaustionAt: null, projectedBeforeReset: null }),
      analysis(),
      analysis({ projectedExhaustionAt: "2026-08-31T20:00:00Z", projectedBeforeReset: false }),
      analysis({ status: "no_recent_change", recentBurnBpsPerHour: 0, projectedExhaustionAt: null, projectedBeforeReset: null }),
      analysis({ status: "stale", stale: true, confidence: "low", projectedExhaustionAt: null, projectedBeforeReset: null }),
      analysis({ status: "limit_reached", utilizationBps: 10_000, remainingBps: 0, projectedExhaustionAt: null, projectedBeforeReset: null }),
    ];
    const expected = ["Gathering pace data", "+8.4 pts/hr", "On pace to stay below limit", "No recent change", "Pace based on previous readings", "Limit reached"];
    cases.forEach((item, index) => {
      const view = render(<Overview data={snapshot()} quotas={[quota({ windows: [quota().windows[0]!], analyses: [item] })]} scanning={false} onScan={vi.fn()} />);
      expect(view.container).toHaveTextContent(expected[index]!);
      if (index === 1) expect(view.container).toHaveTextContent("Likely to reach limit in ~4h 15m");
      view.unmount();
    });
    vi.useRealTimers();
  });

  it("renders Claude and Grok quota pace together and keeps products informational", () => {
    const product = analysis({ provider: "grok", windowKey: "product_chat", label: "Chat", kind: "product", capBearing: false, status: "informational", projectedExhaustionAt: null, projectedBeforeReset: null });
    const grok = quota({
      provider: "grok",
      windows: [{ key: "product_chat", label: "Chat", kind: "product", scope: "PRODUCT_CHAT", utilizationBps: 2_600, periodStartsAt: null, resetsAt: null }],
      analyses: [product],
    });
    const claude = quota({ analyses: [analysis()] });
    render(<Insights insights={[]} byModel={[]} byProject={[]} quotas={[claude, grok]} />);
    expect(screen.getByRole("heading", { name: "Quota pace" })).toBeInTheDocument();
    expect(screen.getByText("Claude 5-hour")).toBeInTheDocument();
    expect(screen.getByText("Grok Chat")).toBeInTheDocument();
    expect(screen.getByText(/no independent cap ETA/)).toBeInTheDocument();
    expect(screen.getByText("Informational only")).toBeInTheDocument();
    expect(document.body.textContent).not.toMatch(/Chat limit|reach Chat/i);
  });

  it("shows healthy Claude live limits, precise percentages, reset countdown, model scope, and separate extra usage", () => {
    vi.useFakeTimers();
    vi.setSystemTime(new Date("2026-08-31T12:00:00Z"));
    render(<Overview data={snapshot()} quotas={[quota({ source: "cloud_sync" })]} scanning={false} onScan={vi.fn()} />);
    expect(screen.getByRole("heading", { name: "Claude live limits" })).toBeInTheDocument();
    expect(screen.getByText("47.63%", { exact: false })).toBeInTheDocument();
    expect(screen.getByText("Resets in 2h 14m")).toBeInTheDocument();
    expect(screen.getByText("Fable")).toBeInTheDocument();
    expect(screen.getByText("Extra usage")).toBeInTheDocument();
    expect(screen.getByText("$12.42")).toBeInTheDocument();
    expect(screen.getByText(/from MacBook/)).toBeInTheDocument();
    vi.useRealTimers();
  });

  it("keeps stale limits visible when Claude is rate limited", () => {
    render(<Overview data={snapshot()} quotas={[quota({ status: "rate_limited", stale: true, message: "Temporarily rate limited. Last good limits remain visible." })]} scanning={false} onScan={vi.fn()} />);
    expect(screen.getByText("Temporarily rate limited. Last good limits remain visible.")).toBeInTheDocument();
    expect(screen.getByText("47.63%", { exact: false })).toBeInTheDocument();
  });

  it("shows Grok weekly and dynamic product quota separately from local telemetry", () => {
    const grok = quota({
      provider: "grok",
      message: "Connected through Grok CLI.",
      planLabel: "SuperGrok Heavy",
      source: "grok_cli",
      windows: [
        { key: "weekly_pool", label: "Weekly", kind: "weekly", scope: "USAGE_PERIOD_TYPE_WEEKLY", utilizationBps: 6_320, periodStartsAt: "2026-08-28T00:00:00Z", resetsAt: "2026-09-04T00:00:00Z" },
        { key: "product_product_grok_build", label: "Grok Build", kind: "product", scope: "PRODUCT_GROK_BUILD", utilizationBps: 4_100, periodStartsAt: null, resetsAt: null },
        { key: "product_product_future_thing", label: "Future Thing", kind: "product", scope: "PRODUCT_FUTURE_THING", utilizationBps: 725, periodStartsAt: null, resetsAt: null },
      ],
      extraUsage: { enabled: true, monthlyLimitMinor: 5_000, usedCreditsMinor: 300, prepaidBalanceMinor: 938, utilizationBps: 600, currency: "USD" },
    });
    render(<Overview data={snapshot()} quotas={[grok]} scanning={false} onScan={vi.fn()} />);
    expect(screen.getByRole("heading", { name: "Grok live limits" })).toBeInTheDocument();
    expect(screen.getByText("SuperGrok Heavy")).toBeInTheDocument();
    expect(screen.getByText("Grok Build")).toBeInTheDocument();
    expect(screen.getByText("Future Thing")).toBeInTheDocument();
    expect(screen.getByText("$9.38")).toBeInTheDocument();
    expect(screen.getByText(/provider-defined quota/)).toBeInTheDocument();
  });

  it("shows no-credential and expired-login states without token details", () => {
    const data = snapshot();
    const baseProps = {
      data,
      scanning: false,
      onScan: vi.fn(() => Promise.resolve()),
      onSync: vi.fn(() => Promise.resolve()),
      onSaveSubscription: vi.fn(() => Promise.resolve()),
      onRenameDevice: vi.fn(() => Promise.resolve(data.device)),
      onToggleClaudeQuota: vi.fn(() => Promise.resolve()),
      onRefreshClaudeQuota: vi.fn(() => Promise.resolve()),
    };
    const view = render(<Settings {...baseProps} claudeQuota={quota({ status: "credential_unavailable", message: "Claude Code sign-in not found. Open Claude Code to sign in.", windows: [], observedAt: null })} />);
    expect(screen.getByText(/Claude Code sign-in not found/)).toBeInTheDocument();
    expect(document.body.textContent).not.toMatch(/Bearer|accessToken|sk-ant/);
    view.rerender(<Settings {...baseProps} claudeQuota={quota({ status: "expired_login", message: "Claude Code sign-in expired. Open Claude Code to refresh your sign-in.", windows: [] })} />);
    expect(screen.getByText(/sign-in expired/)).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Refresh now" }));
    expect(baseProps.onRefreshClaudeQuota).toHaveBeenCalledOnce();
  });

  it("shows truthful onboarding instead of production demo analytics", () => {
    render(<Overview data={snapshot()} scanning={false} onScan={vi.fn()} />);
    expect(screen.getByRole("heading", { name: "Connect your AI usage" })).toBeInTheDocument();
    expect(screen.getByText("Detected · awaiting measured usage")).toBeInTheDocument();
    expect(screen.getByText("Not detected")).toBeInTheDocument();
  });

  it("marks API-equivalent value unavailable when no measured model has safe pricing", () => {
    const data = snapshot();
    data.metrics.measuredEventsRange = 1;
    data.metrics.measuredTokensMonth = 125_000;
    data.metrics.measuredTokensRange = 125_000;
    render(<Overview data={data} scanning={false} onScan={vi.fn()} />);
    expect(screen.getByText("No measured events have safe model pricing")).toBeInTheDocument();
    expect(screen.getByText("Pricing coverage").nextElementSibling).toHaveTextContent("0%");
  });

  it("shows a safe priced subtotal and coverage when only part of the usage can be priced", () => {
    const data = snapshot();
    data.metrics.measuredEventsRange = 10;
    data.metrics.pricedEventsRange = 8;
    data.metrics.measuredTokensMonth = 1_000_000;
    data.metrics.measuredTokensRange = 1_000_000;
    data.metrics.pricedTokensRange = 920_000;
    data.metrics.estimatedApiValueUsdMicros = 18_420_000;
    const view = render(<Overview data={data} scanning={false} onScan={vi.fn()} />);
    const result = within(view.container);
    expect(result.getByText("$18+")).toBeInTheDocument();
    expect(result.getByText("Partial estimate · 92% of measured tokens priced")).toBeInTheDocument();
    expect(result.getByText("Pricing coverage").nextElementSibling).toHaveTextContent("92%");
    expect(result.getByText("$37+")).toBeInTheDocument();
    fireEvent.change(result.getByLabelText("Scenario multiplier"), { target: { value: "5" } });
    expect(result.getByText("$92+")).toBeInTheDocument();
  });

  it("renders measured activity and exposes token parts without conversation content", () => {
    const onLoadMore = vi.fn(() => Promise.resolve());
    render(
      <Activity
        hasMore
        onLoadMore={onLoadMore}
        items={[
          {
            id: "a".repeat(64),
            provider: "codex",
            source: "codex_cli",
            occurredAt: now,
            model: "gpt-5.6-sol",
            projectName: "ArcMeter",
            totalTokens: 1_250,
            inputTokens: 1_000,
            cachedInputTokens: 600,
            cacheWriteTokens: 0,
            cacheWrite5mTokens: 0,
            cacheWrite1hTokens: 0,
            outputTokens: 250,
            reasoningTokens: 80,
            nativeCostUsdTicks: null,
            estimatedApiValueUsdMicros: 2_000,
            measurementKind: "measured",
            deviceId: "device-1",
            deviceName: "Windows Business Desktop",
          },
        ]}
      />,
    );
    fireEvent.click(screen.getByRole("button", { expanded: false }));
    expect(screen.getByText("Cached input")).toBeInTheDocument();
    expect(screen.getByText("Deterministic event", { exact: false })).toBeInTheDocument();
    expect(screen.queryByText(/prompt content/i)).not.toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Load older activity" }));
    expect(onLoadMore).toHaveBeenCalledOnce();
  });

  it("labels activity-only minutes without inventing token telemetry", () => {
    render(
      <Activity items={[{
        id: "b".repeat(64),
        provider: "grok",
        source: "grok_web",
        occurredAt: now,
        model: null,
        projectName: null,
        totalTokens: 0,
        inputTokens: 0,
        cachedInputTokens: 0,
        cacheWriteTokens: 0,
        cacheWrite5mTokens: 0,
        cacheWrite1hTokens: 0,
        outputTokens: 0,
        reasoningTokens: 0,
        nativeCostUsdTicks: null,
        estimatedApiValueUsdMicros: null,
        measurementKind: "activity_only",
        deviceId: "device-1",
        deviceName: "Mac",
      }]} />,
    );
    expect(screen.getByText("Grok web")).toBeInTheDocument();
    expect(screen.getByText("Token telemetry unavailable")).toBeInTheDocument();
    expect(screen.getByText("1 min")).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { expanded: false }));
    expect(screen.getByText(/no URL, title, prompt, response, or token count stored/i)).toBeInTheDocument();
  });

  it("shows Codex aggregate cache writes without inventing a TTL", () => {
    render(<Activity items={[{
      id: "c".repeat(64),
      provider: "codex",
      source: "codex_cli",
      occurredAt: now,
      model: "gpt-5.6-sol",
      projectName: "ArcMeter",
      totalTokens: 120,
      inputTokens: 100,
      cachedInputTokens: 60,
      cacheWriteTokens: 20,
      cacheWrite5mTokens: 0,
      cacheWrite1hTokens: 0,
      outputTokens: 20,
      reasoningTokens: 5,
      nativeCostUsdTicks: null,
      estimatedApiValueUsdMicros: null,
      measurementKind: "measured",
      deviceId: "device-1",
      deviceName: "Windows Business Desktop",
    }]} />);
    fireEvent.click(screen.getByRole("button", { expanded: false }));
    expect(screen.getByText("Cache write")).toBeInTheDocument();
    expect(screen.queryByText(/Cache write \((5m|1h)\)/)).not.toBeInTheDocument();
  });

  it("labels Grok subset counters and recorded cost without calling it an estimate", () => {
    render(
      <Activity items={[{
        id: "c".repeat(64),
        provider: "grok",
        source: "grok_build",
        occurredAt: now,
        model: "grok-4.5-build",
        projectName: "ArcMeter",
        totalTokens: 120,
        inputTokens: 100,
        cachedInputTokens: 40,
        cacheWriteTokens: 10,
        cacheWrite5mTokens: 0,
        cacheWrite1hTokens: 0,
        outputTokens: 20,
        reasoningTokens: 8,
        nativeCostUsdTicks: 120_000_000,
        estimatedApiValueUsdMicros: null,
        measurementKind: "measured",
        deviceId: "device-1",
        deviceName: "Windows Business Desktop",
      }]} />,
    );
    fireEvent.click(screen.getByRole("button", { expanded: false }));
    expect(screen.getByText("Input (cache included)")).toBeInTheDocument();
    expect(screen.getByText("Cache write")).toBeInTheDocument();
    expect(screen.getByText("Reasoning (included in output)")).toBeInTheDocument();
    expect(screen.getByText("Recorded provider cost")).toBeInTheDocument();
    expect(screen.getByText("$0.012")).toBeInTheDocument();
    expect(screen.queryByText("Estimated API-equivalent value")).not.toBeInTheDocument();
  });

  it("labels Claude additive cache counters without double-counting output reasoning", () => {
    render(
      <Activity items={[{
        id: "d".repeat(64),
        provider: "claude",
        source: "claude_code",
        occurredAt: now,
        model: "claude-sonnet-5-20260801",
        projectName: "ArcMeterFixture",
        totalTokens: 39,
        inputTokens: 6,
        cachedInputTokens: 10,
        cacheWriteTokens: 12,
        cacheWrite5mTokens: 8,
        cacheWrite1hTokens: 4,
        outputTokens: 11,
        reasoningTokens: 2,
        nativeCostUsdTicks: null,
        estimatedApiValueUsdMicros: 101,
        measurementKind: "measured",
        deviceId: "device-1",
        deviceName: "Windows Business Desktop",
      }]} />,
    );
    fireEvent.click(screen.getByRole("button", { expanded: false }));
    expect(screen.getByText("Fresh input (cache separate)")).toBeInTheDocument();
    expect(screen.getByText("Cache read")).toBeInTheDocument();
    expect(screen.getByText("Cache write total")).toBeInTheDocument();
    expect(screen.getByText("Cache write (5m)")).toBeInTheDocument();
    expect(screen.getByText("Cache write (1h)")).toBeInTheDocument();
    expect(screen.getByText("Reasoning (included in output)")).toBeInTheDocument();
  });

  it("shows Claude request and cache diagnostics separately from Claude Desktop", () => {
    const data = snapshot();
    data.sources[1] = {
      ...data.sources[1]!,
      detected: true,
      measuredRecords: 1_842,
      measuredSessions: 37,
      measuredTokens: 91_600_000,
      cacheReadTokens: 54_200_000,
      cacheWriteTokens: 8_700_000,
      lastUsageAt: now,
    };
    render(
      <Settings
        data={data}
        scanning={false}
        onScan={vi.fn(() => Promise.resolve())}
        onSync={vi.fn(() => Promise.resolve())}
        onSaveSubscription={vi.fn(() => Promise.resolve())}
        onRenameDevice={vi.fn(() => Promise.resolve(data.device))}
      />,
    );
    const card = screen.getByText("Claude Code CLI").closest("article");
    expect(card).not.toBeNull();
    expect(within(card!).getByText("Sessions").nextElementSibling).toHaveTextContent("37");
    expect(within(card!).getByText("Requests").nextElementSibling).toHaveTextContent("1,842");
    expect(within(card!).getByText("Cache reads").nextElementSibling).toHaveTextContent("54.2M");
    expect(within(card!).getByText("Cache writes").nextElementSibling).toHaveTextContent("8.7M");
    expect(screen.getByText("Claude Desktop")).toBeInTheDocument();
  });

  it("shows Grok session, turn, token, and native-cost diagnostics", () => {
    const data = snapshot();
    data.sources.push({
      provider: "grok",
      label: "Grok Build",
      detected: true,
      filesSeen: 2,
      recordsSeen: 20,
      recordsInserted: 4,
      measuredRecords: 5,
      measuredSessions: 2,
      measuredTurns: 4,
      measuredTokens: 460,
      cacheReadTokens: 100,
      cacheWriteTokens: 20,
      nativeCostUsdTicks: 270_000_000,
      lastScanAt: now,
      lastUsageAt: now,
      status: "healthy",
      diagnostics: [],
    });
    render(
      <Settings
        data={data}
        scanning={false}
        onScan={vi.fn(() => Promise.resolve())}
        onSync={vi.fn(() => Promise.resolve())}
        onSaveSubscription={vi.fn(() => Promise.resolve())}
        onRenameDevice={vi.fn(() => Promise.resolve(data.device))}
      />,
    );
    const card = screen.getByText("Grok Build").closest("article");
    expect(card).not.toBeNull();
    expect(within(card!).getByText("Sessions").nextElementSibling).toHaveTextContent("2");
    expect(within(card!).getByText("Measured turns").nextElementSibling).toHaveTextContent("4");
    expect(within(card!).getByText("Recorded native cost").nextElementSibling).toHaveTextContent("$0.027");
  });

  it("surfaces collector diagnostics without revealing a filesystem path", () => {
    const data = snapshot();
    data.sources[0] = {
      ...data.sources[0]!,
      status: "error",
      diagnostics: [
        {
          severity: "error",
          code: "source_unreadable",
          message: "ArcMeter could not read a Codex session file",
          recordNumber: null,
        },
      ],
    };
    render(
      <Settings
        data={data}
        scanning={false}
        onScan={vi.fn(() => Promise.resolve())}
        onSync={vi.fn(() => Promise.resolve())}
        onSaveSubscription={vi.fn(() => Promise.resolve())}
        onRenameDevice={vi.fn(() => Promise.resolve(data.device))}
      />,
    );
    expect(screen.getByText("ArcMeter could not read a Codex session file")).toBeInTheDocument();
    expect(screen.getByText("Cloud account not configured")).toBeInTheDocument();
    expect(document.body.textContent).not.toContain("C:\\Users");
    expect(screen.getAllByText("Claude Code CLI").length).toBeGreaterThan(0);
    expect(screen.getByText(/CLI telemetry only/)).toBeInTheDocument();
    const desktopCard = screen.getByText("Claude Desktop").closest("article");
    expect(desktopCard).not.toBeNull();
    expect(within(desktopCard!).getByText("Foreground activity only · no token telemetry")).toBeInTheDocument();
    expect(within(desktopCard!).getByText("0 min")).toBeInTheDocument();
    expect(within(desktopCard!).getByText("Unavailable")).toBeInTheDocument();
  });

  it("groups measured sessions, keeps native IDs opaque, and loads detailed token semantics on demand", async () => {
    vi.useFakeTimers({ toFake: ["Date"] });
    vi.setSystemTime(new Date("2026-09-01T18:00:00Z"));
    const page = sessionPage();
    const loadDetail = vi.fn(() => Promise.resolve(sessionDetail(page)));
    render(<Sessions initialPage={page} loadDetail={loadDetail} />);
    expect(screen.getByRole("heading", { name: "Today" })).toBeInTheDocument();
    expect(screen.getByRole("heading", { name: "Older" })).toBeInTheDocument();
    expect(screen.getByText("Measured tokens")).toBeInTheDocument();
    expect(screen.getByText("1.3K")).toBeInTheDocument();
    expect(document.body.textContent).not.toContain("do-not-display-claude-session");
    expect(document.body.textContent).not.toContain("do-not-display-grok-session");
    fireEvent.click(screen.getByRole("button", { name: /ArcMeter.*Claude Code/i }));
    expect(await screen.findByText("Token composition")).toBeInTheDocument();
    expect(loadDetail).toHaveBeenCalledWith(page.sessions[0]);
    expect(screen.getByText("Fresh input (cache separate)")).toBeInTheDocument();
    expect(screen.getByText("Reasoning (included in output)")).toBeInTheDocument();
    expect(screen.getByText("Partial pricing coverage; priced subtotal excludes unavailable components.")).toBeInTheDocument();
    expect(screen.getByText("Recorded provider cost")).toBeInTheDocument();
    expect(screen.getByText("MacBook")).toBeInTheDocument();
  });

  it("sends provider, date, project search, and sort filters to the bounded native session query", async () => {
    const page = sessionPage();
    const loadPage = vi.fn(() => Promise.resolve({ ...page, sessions: [page.sessions[0]!] }));
    render(<Sessions initialPage={page} loadPage={loadPage} />);
    fireEvent.change(screen.getByLabelText("Provider"), { target: { value: "claude" } });
    await waitFor(() => expect(loadPage).toHaveBeenLastCalledWith(expect.objectContaining({ provider: "claude", range: "30d", sort: "recent", limit: 50, offset: 0 })));
    fireEvent.change(screen.getByLabelText("Date range"), { target: { value: "7d" } });
    await waitFor(() => expect(loadPage).toHaveBeenLastCalledWith(expect.objectContaining({ provider: "claude", range: "7d" })));
    fireEvent.change(screen.getByLabelText("Search projects"), { target: { value: "ArcMeter" } });
    await waitFor(() => expect(loadPage).toHaveBeenLastCalledWith(expect.objectContaining({ search: "ArcMeter" })));
    fireEvent.change(screen.getByLabelText("Sort sessions"), { target: { value: "tokens" } });
    await waitFor(() => expect(loadPage).toHaveBeenLastCalledWith(expect.objectContaining({ sort: "tokens" })));
  });

  it("distinguishes a first-use session empty state from filtered empty results", async () => {
    const empty = sessionPage({ sessions: [], totalCount: 0, stats: { sessionCount: 0, totalTokens: 0, estimatedApiValueUsdMicros: null } });
    const view = render(<Sessions initialPage={empty} />);
    expect(screen.getByRole("heading", { name: "No measured sessions yet" })).toBeInTheDocument();
    view.rerender(<Sessions initialPage={empty} />);
    fireEvent.change(screen.getByLabelText("Search projects"), { target: { value: "ArcMeter" } });
    expect(await screen.findByRole("heading", { name: "No sessions match these filters" })).toBeInTheDocument();
  });
});
