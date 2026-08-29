import { Download, RefreshCw, ShieldCheck } from "lucide-react";
import { createContext, useCallback, useContext, useEffect, useMemo, useRef, useState } from "react";
import type { AvailableUpdate, UpdaterService } from "../lib/updater";
import { tauriUpdaterService } from "../lib/updater";

type UpdatePhase = "unavailable" | "checking" | "current" | "available" | "installing" | "restarting" | "error";

interface UpdaterState {
  phase: UpdatePhase;
  version: string | null;
  progressPercent: number | null;
  checkNow: () => Promise<void>;
  install: () => Promise<void>;
}

const noUpdater: UpdaterState = {
  phase: "unavailable",
  version: null,
  progressPercent: null,
  checkNow: () => Promise.resolve(),
  install: () => Promise.resolve(),
};

const UpdaterContext = createContext<UpdaterState>(noUpdater);

export function AppUpdaterProvider({
  children,
  checkDelayMs = 1_500,
  os,
  service = tauriUpdaterService,
}: {
  children: React.ReactNode;
  checkDelayMs?: number;
  os: string | null;
  service?: UpdaterService;
}) {
  const [phase, setPhase] = useState<UpdatePhase>(service.isSupported() ? "checking" : "unavailable");
  const [version, setVersion] = useState<string | null>(null);
  const [progressPercent, setProgressPercent] = useState<number | null>(null);
  const updateRef = useRef<AvailableUpdate | null>(null);
  const installingRef = useRef(false);

  const checkNow = useCallback(async () => {
    if (!service.isSupported() || installingRef.current) return;
    setPhase("checking");
    setProgressPercent(null);
    try {
      const previous = updateRef.current;
      const next = await service.check();
      updateRef.current = next;
      if (previous && previous !== next) void previous.close().catch(() => undefined);
      setVersion(next?.version ?? null);
      setPhase(next ? "available" : "current");
    } catch {
      setPhase("error");
    }
  }, [service]);

  const install = useCallback(async () => {
    const update = updateRef.current;
    if (!update || installingRef.current) return;
    installingRef.current = true;
    setPhase("installing");
    setProgressPercent(0);
    try {
      await update.downloadAndInstall(({ downloadedBytes, totalBytes }) => {
        setProgressPercent(totalBytes && totalBytes > 0 ? Math.min(100, Math.round((downloadedBytes / totalBytes) * 100)) : null);
      });
      setPhase("restarting");
      setProgressPercent(100);
      if (os === "macos") await service.relaunch();
    } catch {
      installingRef.current = false;
      setPhase("error");
    }
  }, [os, service]);

  useEffect(() => {
    if (!service.isSupported()) return;
    const timer = window.setTimeout(() => { void checkNow(); }, checkDelayMs);
    return () => window.clearTimeout(timer);
  }, [checkDelayMs, checkNow, service]);

  useEffect(() => () => {
    if (!installingRef.current) void updateRef.current?.close().catch(() => undefined);
  }, []);

  const value = useMemo(() => ({ phase, version, progressPercent, checkNow, install }), [checkNow, install, phase, progressPercent, version]);

  return <UpdaterContext.Provider value={value}>{children}</UpdaterContext.Provider>;
}

export function UpdateBanner({ onOpenSettings }: { onOpenSettings: () => void }) {
  const updater = useContext(UpdaterContext);
  if (updater.phase !== "available") return null;
  return (
    <div className="update-banner" role="status">
      <Download />
      <div><strong>ArcMeter {updater.version} is ready</strong><span>The update is signed and will install only after you approve it.</span></div>
      <button type="button" onClick={onOpenSettings}>Review update</button>
    </div>
  );
}

export function UpdateSettingRow({ currentVersion }: { currentVersion: string }) {
  const updater = useContext(UpdaterContext);
  const detail = updateDetail(updater, currentVersion);
  const busy = updater.phase === "checking" || updater.phase === "installing" || updater.phase === "restarting";
  const installReady = updater.phase === "available";
  const buttonLabel = updater.phase === "checking"
    ? "Checking"
    : updater.phase === "installing"
      ? updater.progressPercent === null ? "Downloading" : `Downloading ${updater.progressPercent}%`
      : updater.phase === "restarting"
        ? "Restarting"
        : installReady
          ? `Install ${updater.version}`
          : updater.phase === "error"
            ? "Retry"
            : "Check now";

  return (
    <div className="setting-row update-setting-row">
      <span className="setting-row-icon"><ShieldCheck /></span>
      <div>
        <strong>Signed updates</strong>
        <p>{detail}</p>
        {updater.phase === "installing" ? <progress aria-label="Update download progress" max="100" value={updater.progressPercent ?? undefined} /> : null}
      </div>
      <button
        type="button"
        className="secondary-button"
        disabled={busy || updater.phase === "unavailable"}
        onClick={() => void (installReady ? updater.install() : updater.checkNow())}
      >
        {updater.phase === "checking" || updater.phase === "installing" ? <RefreshCw className="spin" /> : installReady ? <Download /> : null}
        {buttonLabel}
      </button>
    </div>
  );
}

function updateDetail(updater: UpdaterState, currentVersion: string): string {
  if (updater.phase === "unavailable") return "Automatic signed updates are available in packaged Windows and macOS builds.";
  if (updater.phase === "checking") return `Checking ArcMeter ${currentVersion} against the signed release channel.`;
  if (updater.phase === "current") return `ArcMeter ${currentVersion} is current. New signed releases are checked automatically.`;
  if (updater.phase === "available") return `ArcMeter ${updater.version} is available. Installing verifies its signature before replacing this version.`;
  if (updater.phase === "installing") return "Downloading and verifying the signed update. Keep ArcMeter open.";
  if (updater.phase === "restarting") return "The update is installed. ArcMeter is restarting to finish.";
  return "ArcMeter could not reach or verify the update channel. Your current installation was not changed.";
}
