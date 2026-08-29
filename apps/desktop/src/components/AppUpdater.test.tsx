import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import type { AvailableUpdate, UpdaterService } from "../lib/updater";
import { AppUpdaterProvider, UpdateBanner, UpdateSettingRow } from "./AppUpdater";

describe("signed application updates", () => {
  it("announces an available update and installs it only after approval", async () => {
    const relaunch = vi.fn(() => Promise.resolve());
    const downloadAndInstall = vi.fn((onProgress: Parameters<AvailableUpdate["downloadAndInstall"]>[0]) => {
      onProgress({ downloadedBytes: 50, totalBytes: 100 });
      onProgress({ downloadedBytes: 100, totalBytes: 100 });
      return Promise.resolve();
    });
    const update: AvailableUpdate = {
      version: "0.3.0",
      date: "2026-08-29T00:00:00Z",
      notes: "A safer update.",
      downloadAndInstall,
      close: vi.fn(() => Promise.resolve()),
    };
    const service: UpdaterService = {
      isSupported: () => true,
      check: vi.fn(() => Promise.resolve(update)),
      relaunch,
    };

    render(
      <AppUpdaterProvider checkDelayMs={0} os="macos" service={service}>
        <UpdateBanner onOpenSettings={vi.fn()} />
        <UpdateSettingRow currentVersion="0.2.0" />
      </AppUpdaterProvider>,
    );

    expect(await screen.findByText("ArcMeter 0.3.0 is ready")).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Install 0.3.0" }));

    await waitFor(() => expect(downloadAndInstall).toHaveBeenCalledOnce());
    await waitFor(() => expect(relaunch).toHaveBeenCalledOnce());
  });

  it("does not offer native updating in a browser preview", () => {
    const service: UpdaterService = {
      isSupported: () => false,
      check: vi.fn(() => Promise.resolve(null)),
      relaunch: vi.fn(() => Promise.resolve()),
    };
    render(
      <AppUpdaterProvider os="windows" service={service}>
        <UpdateSettingRow currentVersion="0.2.0" />
      </AppUpdaterProvider>,
    );
    expect(screen.getByRole("button", { name: "Check now" })).toBeDisabled();
    expect(service.check).not.toHaveBeenCalled();
  });
});
