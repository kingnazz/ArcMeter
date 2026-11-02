import { invoke } from "@tauri-apps/api/core";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { SESSION_EXPIRED_MESSAGE, SessionExpiredError, syncCloudNow } from "./api";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn(), isTauri: () => true }));

describe("native sync error contract", () => {
  beforeEach(() => { vi.mocked(invoke).mockReset(); });

  it("turns the session-expired code into a friendly typed error", async () => {
    vi.mocked(invoke).mockRejectedValue({ code: "session_expired", message: "internal service details" });
    await expect(syncCloudNow()).rejects.toBeInstanceOf(SessionExpiredError);
    await expect(syncCloudNow()).rejects.toThrow(SESSION_EXPIRED_MESSAGE);
  });

  it("preserves temporary failures without requesting reauthentication", async () => {
    vi.mocked(invoke).mockRejectedValue({ code: "sync_failed", message: "Authentication service error: HTTP 503" });
    const error = await syncCloudNow().catch((reason: unknown) => reason);
    expect(error).toBeInstanceOf(Error);
    expect(error).not.toBeInstanceOf(SessionExpiredError);
    expect((error as Error).message).toBe("Authentication service error: HTTP 503");
  });

  it("does not infer expiration from legacy HTTP message strings", async () => {
    vi.mocked(invoke).mockRejectedValue("HTTP 400");
    await expect(syncCloudNow()).rejects.toBe("HTTP 400");
  });

  it("returns successful sync reports unchanged", async () => {
    const report = { uploadedEvents: 1, downloadedEvents: 2, downloadedDevices: 1, syncedSubscriptions: 0, completedAt: "2026-11-02T00:00:00Z" };
    vi.mocked(invoke).mockResolvedValue(report);
    await expect(syncCloudNow()).resolves.toEqual(report);
    expect(invoke).toHaveBeenCalledExactlyOnceWith("sync_now");
  });
});
