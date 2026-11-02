import { act, cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { invoke } from "@tauri-apps/api/core";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { getAuthStatus, getDashboard, SESSION_EXPIRED_MESSAGE, syncCloudNow } from "../lib/api";
import type { AuthStatus, DashboardSnapshot } from "../types";
import { Settings } from "./Settings";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn(), isTauri: () => true }));
vi.mock("@tauri-apps/plugin-autostart", () => ({
  isEnabled: () => Promise.resolve(false), enable: vi.fn(), disable: vi.fn(),
}));
vi.mock("./AppUpdater", () => ({ UpdateSettingRow: () => null }));

const connected: AuthStatus = { configured: true, signedIn: true, email: "test@example.com", expiresAt: "2026-11-02T12:00:00Z" };
const signedOut: AuthStatus = { configured: true, signedIn: false, email: null, expiresAt: null };

let nativeStatus: AuthStatus;
let syncFailure: unknown;
let statusFailure: boolean;

afterEach(cleanup);

async function renderSettings() {
  // Exercise the real API adapter with an isolated metadata-only native fixture.
  const data: DashboardSnapshot = await getDashboard("month");
  const onSync = vi.fn(async () => { await syncCloudNow(); });
  const props = {
    data, initialSection: "sync", scanning: false, onScan: vi.fn(() => Promise.resolve()), onSync,
    onSaveSubscription: vi.fn(() => Promise.resolve()), onRenameDevice: vi.fn(() => Promise.resolve(data.device)),
  };
  const view = render(<Settings {...props} />);
  await screen.findByText(/Connected as test@example.com/);
  return { ...view, props, onSync };
}

// Only public account metadata crosses the mocked IPC boundary.
beforeEach(() => {
  nativeStatus = connected;
  syncFailure = undefined;
  statusFailure = false;
  vi.mocked(invoke).mockReset();
  vi.mocked(invoke).mockImplementation((command) => {
    if (command === "dashboard_snapshot") return Promise.resolve({
      generatedAt: "2026-11-02T00:00:00Z", range: "month", metrics: {}, localModels: { runtimes: [], measuredTokensToday: 0, requestsToday: null, observationsToday: 0, historyStatus: "unavailable" }, trend: [], byProvider: [], byModel: [], byProject: [], byDevice: [], activity: [], insights: [], sources: [], subscriptions: [],
      device: { id: "test-device", friendlyName: "Test Mac", os: "macos", architecture: "aarch64", appVersion: "0.2.0", createdAt: "2026-11-02T00:00:00Z", lastSeenAt: "2026-11-02T00:00:00Z", lastSyncAt: null, syncStatus: "local_only" },
    });
    if (command === "auth_status") {
      if (statusFailure) return Promise.reject(new Error("IPC unavailable"));
      return Promise.resolve(nativeStatus);
    }
    if (command === "sync_now") {
      if (syncFailure) {
        if ((syncFailure as { code?: string }).code === "session_expired") nativeStatus = signedOut;
        // Tauri rejects IPC calls with serialized objects, rather than Error instances.
        // eslint-disable-next-line @typescript-eslint/prefer-promise-reject-errors
        return Promise.reject(syncFailure);
      }
      return Promise.resolve({});
    }
    if (command === "auth_sign_in") { nativeStatus = connected; return Promise.resolve(connected); }
    return Promise.resolve(null);
  });
});

describe("Settings account recovery", () => {
  it("immediately replaces a rejected connected account with sign-in and an error toast", async () => {
    const { onSync } = await renderSettings();
    syncFailure = { code: "session_expired", message: "Session refresh returned HTTP 400 Bad Request" };
    fireEvent.click(screen.getByRole("button", { name: "Sync now" }));
    expect(await screen.findByRole("button", { name: "Sign in" })).toBeEnabled();
    expect(screen.getByPlaceholderText("Email")).toBeInTheDocument();
    expect(screen.queryByText(/Connected as/)).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Sync now" })).toBeDisabled();
    const alert = screen.getByRole("alert");
    expect(alert).toHaveTextContent(SESSION_EXPIRED_MESSAGE);
    expect(alert).toHaveClass("toast-error");
    expect(alert.querySelector(".lucide-triangle-alert")).toBeInTheDocument();
    expect(alert.querySelector(".lucide-check")).not.toBeInTheDocument();
    expect(document.body).not.toHaveTextContent("HTTP 400");
    await waitFor(() => expect(vi.mocked(invoke).mock.calls.filter(([command]) => command === "auth_status")).toHaveLength(2));
    expect(onSync).toHaveBeenCalledOnce();
    fireEvent.click(screen.getByRole("button", { name: "Sync now" }));
    expect(onSync).toHaveBeenCalledOnce();

    fireEvent.change(screen.getByPlaceholderText("Email"), { target: { value: "test@example.com" } });
    fireEvent.change(screen.getByPlaceholderText("Password"), { target: { value: "synthetic-test-password" } });
    fireEvent.click(screen.getByRole("button", { name: "Sign in" }));
    expect(await screen.findByText(/Connected as test@example.com/)).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Sync now" })).toBeEnabled();
    expect(screen.queryByText(SESSION_EXPIRED_MESSAGE)).not.toBeInTheDocument();
  });

  it("keeps a session connected after temporary failures and uses error semantics", async () => {
    await renderSettings();
    syncFailure = { code: "sync_failed", message: "Cloud service temporarily unavailable" };
    fireEvent.click(screen.getByRole("button", { name: "Sync now" }));
    expect(await screen.findByRole("alert")).toHaveTextContent("Cloud service temporarily unavailable");
    await waitFor(() => expect(screen.getByRole("button", { name: "Sync now" })).toBeEnabled());
    expect(screen.getByText(/Connected as test@example.com/)).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Sign in" })).not.toBeInTheDocument();
    expect(screen.getByRole("alert").querySelector(".lucide-check")).not.toBeInTheDocument();
  });

  it("uses a checkmark and status semantics for a successful sync", async () => {
    await renderSettings();
    fireEvent.click(screen.getByRole("button", { name: "Sync now" }));
    const toast = await screen.findByRole("status");
    expect(toast).toHaveTextContent("Cloud metadata synchronized");
    expect(toast).toHaveClass("toast-success");
    expect(toast.querySelector(".lucide-check")).toBeInTheDocument();
    expect(toast.querySelector(".lucide-triangle-alert")).not.toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Dismiss notification" }));
    expect(screen.queryByRole("status")).not.toBeInTheDocument();
  });

  it("keeps the sign-in form visible if auth status cannot be reloaded after rejection", async () => {
    await renderSettings();
    statusFailure = true;
    syncFailure = { code: "session_expired" };
    fireEvent.click(screen.getByRole("button", { name: "Sync now" }));
    expect(await screen.findByRole("button", { name: "Sign in" })).toBeInTheDocument();
    await waitFor(() => expect(screen.getByRole("alert")).toHaveTextContent(SESSION_EXPIRED_MESSAGE));
    expect(screen.queryByText(/Connected as/)).not.toBeInTheDocument();
  });

  it("rechecks account status when native background sync updates the dashboard", async () => {
    const { rerender, props } = await renderSettings();
    nativeStatus = signedOut;
    rerender(<Settings {...props} data={{ ...props.data, generatedAt: "2026-11-02T00:01:00Z" }} />);
    expect(await screen.findByRole("button", { name: "Sign in" })).toBeInTheDocument();
    expect(screen.queryByText(/Connected as/)).not.toBeInTheDocument();
  });

  it("ignores an older connected status response after a rejected sync", async () => {
    const { rerender, props } = await renderSettings();
    let resolveStale!: (status: AuthStatus) => void;
    const staleStatus = new Promise<AuthStatus>((resolve) => { resolveStale = resolve; });
    const original = vi.mocked(invoke).getMockImplementation()!;
    let pending = true;
    vi.mocked(invoke).mockImplementation((command, args, options) => {
      if (command === "auth_status" && pending) { pending = false; return staleStatus; }
      return original(command, args, options);
    });
    rerender(<Settings {...props} data={{ ...props.data, generatedAt: "2026-11-02T00:01:00Z" }} />);
    syncFailure = { code: "session_expired" };
    fireEvent.click(screen.getByRole("button", { name: "Sync now" }));
    await screen.findByRole("button", { name: "Sign in" });
    await act(async () => { resolveStale(connected); await staleStatus; });
    expect(screen.queryByText(/Connected as/)).not.toBeInTheDocument();
    expect(await getAuthStatus()).toEqual(signedOut);
  });
});
