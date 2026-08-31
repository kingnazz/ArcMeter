import { cleanup, fireEvent, render, screen, within } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import type { DashboardSnapshot } from "../types";
import { Activity } from "./Activity";
import { Overview } from "./Overview";
import { Settings } from "./Settings";

const now = "2026-08-28T07:00:00Z";

afterEach(cleanup);

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

describe("important product states", () => {
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
});
