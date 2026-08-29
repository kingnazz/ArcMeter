import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import type { DashboardSnapshot } from "../types";
import { Activity } from "./Activity";
import { Overview } from "./Overview";
import { Settings } from "./Settings";

const now = "2026-08-28T07:00:00Z";

function snapshot(): DashboardSnapshot {
  return {
    generatedAt: now,
    range: "month",
    metrics: {
      measuredTokensToday: 0,
      measuredTokensMonth: 0,
      measuredTokensRange: 0,
      measuredEventsRange: 0,
      activityMinutesRange: 0,
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
    sources: [
      {
        provider: "codex",
        label: "Codex",
        detected: true,
        filesSeen: 2,
        recordsSeen: 0,
        recordsInserted: 0,
        measuredRecords: 0,
        measuredTokens: 0,
        lastScanAt: now,
        lastUsageAt: null,
        status: "healthy",
        diagnostics: [],
      },
      {
        provider: "claude",
        label: "Claude Code",
        detected: false,
        filesSeen: 0,
        recordsSeen: 0,
        recordsInserted: 0,
        measuredRecords: 0,
        measuredTokens: 0,
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
      appVersion: "0.1.0",
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

  it("marks API-equivalent value unavailable when any model lacks exact pricing", () => {
    const data = snapshot();
    data.metrics.measuredEventsRange = 1;
    data.metrics.measuredTokensMonth = 125_000;
    data.metrics.measuredTokensRange = 125_000;
    render(<Overview data={data} scanning={false} onScan={vi.fn()} />);
    expect(screen.getByText("Pricing unavailable for one or more measured models")).toBeInTheDocument();
    expect(screen.getByText("Value captured").nextElementSibling).toHaveTextContent("—");
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
            outputTokens: 250,
            reasoningTokens: 80,
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
        outputTokens: 0,
        reasoningTokens: 0,
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
  });
});
