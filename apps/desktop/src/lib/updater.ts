import { isTauri } from "@tauri-apps/api/core";
import { relaunch } from "@tauri-apps/plugin-process";
import { check, type DownloadEvent } from "@tauri-apps/plugin-updater";

export interface UpdateDownloadProgress {
  downloadedBytes: number;
  totalBytes: number | null;
}

export interface AvailableUpdate {
  version: string;
  date: string | null;
  notes: string | null;
  downloadAndInstall: (onProgress: (progress: UpdateDownloadProgress) => void) => Promise<void>;
  close: () => Promise<void>;
}

export interface UpdaterService {
  isSupported: () => boolean;
  check: () => Promise<AvailableUpdate | null>;
  relaunch: () => Promise<void>;
}

export const tauriUpdaterService: UpdaterService = {
  isSupported: isTauri,
  async check() {
    const update = await check();
    if (!update) return null;

    return {
      version: update.version,
      date: update.date ?? null,
      notes: update.body ?? null,
      async downloadAndInstall(onProgress) {
        let downloadedBytes = 0;
        let totalBytes: number | null = null;

        await update.downloadAndInstall((event: DownloadEvent) => {
          if (event.event === "Started") {
            totalBytes = event.data.contentLength ?? null;
            onProgress({ downloadedBytes, totalBytes });
          } else if (event.event === "Progress") {
            downloadedBytes += event.data.chunkLength;
            onProgress({ downloadedBytes, totalBytes });
          } else {
            if (totalBytes !== null) downloadedBytes = totalBytes;
            onProgress({ downloadedBytes, totalBytes });
          }
        });
      },
      close: () => update.close(),
    };
  },
  relaunch,
};
