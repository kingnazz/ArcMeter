import { disable, enable, isEnabled } from "@tauri-apps/plugin-autostart";
import { isTauri } from "@tauri-apps/api/core";
import { Activity as ActivityIcon, Check, CloudOff, Copy, Globe2, HardDrive, Laptop, RefreshCw, Save, ShieldCheck, TriangleAlert } from "lucide-react";
import { useEffect, useState } from "react";
import type { ActivityTrackingStatus, AuthStatus, DashboardSnapshot, Device, Subscription } from "../types";
import { formatMinutes, formatRelativeTime, formatTokens, providerLabel } from "../lib/format";
import { getActivityTrackingStatus, getAuthStatus, getSetting, setSetting, signIn, signOut } from "../lib/api";
import { ProviderMark } from "./ProviderMark";
import { UpdateSettingRow } from "./AppUpdater";

interface SettingsProps {
  data: DashboardSnapshot;
  scanning: boolean;
  onScan: () => Promise<void>;
  onSync: () => Promise<void>;
  onSaveSubscription: (subscription: Subscription) => Promise<void>;
  onRenameDevice: (name: string) => Promise<Device>;
}

export function Settings({ data, scanning, onScan, onSync, onSaveSubscription, onRenameDevice }: SettingsProps) {
  const [deviceName, setDeviceName] = useState(data.device.friendlyName);
  const [savingDevice, setSavingDevice] = useState(false);
  const [autostart, setAutostart] = useState(false);
  const [closeToTray, setCloseToTray] = useState(true);
  const [message, setMessage] = useState<string | null>(null);
  const [auth, setAuth] = useState<AuthStatus>({ configured: false, signedIn: false, email: null, expiresAt: null });
  const [syncing, setSyncing] = useState(false);
  const [activityStatus, setActivityStatus] = useState<ActivityTrackingStatus | null>(null);

  useEffect(() => {
    if (!isTauri()) return;
    void isEnabled().then(setAutostart).catch(() => setAutostart(false));
    void getSetting("close_to_tray").then((value) => setCloseToTray(value !== "false"));
    void getAuthStatus().then(setAuth);
  }, []);

  useEffect(() => {
    if (!isTauri()) return;
    void getActivityTrackingStatus().then(setActivityStatus);
  }, [data.generatedAt]);

  async function saveDeviceName() {
    if (deviceName.trim() === data.device.friendlyName || !deviceName.trim()) return;
    setSavingDevice(true);
    try {
      await onRenameDevice(deviceName.trim());
      setMessage("Device name saved");
    } finally {
      setSavingDevice(false);
    }
  }

  async function toggleAutostart(next: boolean) {
    try {
      if (next) await enable(); else await disable();
      setAutostart(next);
    } catch {
      setMessage("Launch-at-login could not be changed on this system");
    }
  }

  async function toggleCloseToTray(next: boolean) {
    await setSetting("close_to_tray", String(next));
    setCloseToTray(next);
  }

  async function syncCloud() {
    setSyncing(true);
    try {
      await onSync();
      setMessage("Cloud metadata synchronized");
    } catch (reason) {
      setMessage(reason instanceof Error ? reason.message : String(reason));
    } finally {
      setSyncing(false);
    }
  }

  async function toggleActivity(key: "activity_claude_desktop_enabled" | "activity_browser_bridge_enabled", next: boolean) {
    await setSetting(key, String(next));
    setActivityStatus(await getActivityTrackingStatus());
    setMessage(next ? "Activity tracking enabled" : "Activity tracking disabled");
  }

  async function copyPairingToken() {
    if (!activityStatus?.pairingToken) return;
    await navigator.clipboard.writeText(activityStatus.pairingToken);
    setMessage("Browser pairing token copied");
  }

  const claudeDesktopStatusLabel = activityStatus
    ? activityStatus.claudeDesktopSupported
      ? activityStatus.claudeDesktopEnabled ? "Tracking" : "Off"
      : "macOS only"
    : data.device.os === "macos" ? "Checking" : "macOS only";

  return (
    <div className="settings-layout">
      {message ? <button className="toast" type="button" onClick={() => setMessage(null)}><Check />{message}</button> : null}
      <section className="settings-section">
        <SettingsHeading title="Account" description="Secure cloud sync across your ArcMeter devices." />
        <AccountPanel auth={auth} onChange={setAuth} />
      </section>

      <section className="settings-section">
        <SettingsHeading title="Devices" description="Each installation has a stable identity independent of its hostname." />
        <div className="settings-panel device-panel">
          <div className="device-avatar"><Laptop /></div>
          <div className="device-main">
            <label>Friendly name<input value={deviceName} maxLength={80} onChange={(event) => setDeviceName(event.target.value)} /></label>
            <div className="device-meta">
              <span>{osLabel(data.device.os)} · {data.device.architecture}</span>
              <span>ArcMeter {data.device.appVersion}</span>
              <span>Seen {formatRelativeTime(data.device.lastSeenAt)}</span>
            </div>
          </div>
          <button type="button" className="icon-button labeled" disabled={savingDevice || deviceName.trim() === data.device.friendlyName} onClick={() => void saveDeviceName()}><Save />Save</button>
          <ConnectionStatus device={data.device} />
        </div>
      </section>

      <section className="settings-section">
        <SettingsHeading title="AI sources" description="Collectors read trusted local telemetry; sensitive content stays outside ArcMeter." action={<button type="button" className="secondary-button" onClick={() => void onScan()} disabled={scanning}><RefreshCw className={scanning ? "spin" : ""} />{scanning ? "Scanning" : "Scan now"}</button>} />
        <div className="source-settings-grid">
          {data.sources.map((source) => (
            <article className="source-setting" key={source.provider}>
              <ProviderMark provider={source.provider} />
              <div className="source-setting-title"><strong>{source.label}</strong><span>{source.provider === "claude" ? `${source.detected ? "Detected" : "Not detected"} · CLI telemetry only` : source.detected ? "Detected" : "Not detected"}</span></div>
              <StatusBadge status={source.detected ? source.status : "idle"} />
              <dl>
                <div><dt>Measured events</dt><dd>{source.measuredRecords.toLocaleString()}</dd></div>
                <div><dt>Measured tokens</dt><dd>{formatTokens(source.measuredTokens)}</dd></div>
                <div><dt>Last usage</dt><dd>{formatRelativeTime(source.lastUsageAt)}</dd></div>
                <div><dt>Last scan</dt><dd>{formatRelativeTime(source.lastScanAt)}</dd></div>
              </dl>
              {source.diagnostics[0] ? <p className="source-diagnostic"><TriangleAlert />{source.diagnostics[0].message}</p> : null}
            </article>
          ))}
          <article className="source-setting">
            <ProviderMark provider="claude" />
            <div className="source-setting-title"><strong>Claude Desktop</strong><span>Foreground activity only · no token telemetry</span></div>
            <StatusBadge status={activityStatus?.claudeDesktopEnabled ? "healthy" : "idle"} label={claudeDesktopStatusLabel} />
            <dl>
              <div><dt>Recorded activity</dt><dd>{formatMinutes(activityStatus?.claudeDesktopMinutes ?? 0)}</dd></div>
              <div><dt>Measured tokens</dt><dd>Unavailable</dd></div>
              <div><dt>Last activity</dt><dd>{formatRelativeTime(activityStatus?.claudeDesktopLastActivityAt ?? null)}</dd></div>
              <div><dt>History</dt><dd>Since enabled</dd></div>
            </dl>
          </article>
        </div>
      </section>

      <section className="settings-section">
        <SettingsHeading title="Activity tracking" description="Optional foreground-time signals for apps that do not expose trustworthy token telemetry." />
        <div className="settings-panel toggles-panel">
          <SettingToggle
            icon={<ActivityIcon />}
            title="Claude Desktop active minutes"
            detail={activityStatus?.claudeDesktopSupported ? "Counts a minute only while Claude Desktop is the frontmost macOS app. No window titles or conversation content are read." : "Available in the macOS build. Enable it there to count foreground Claude Desktop minutes."}
            checked={activityStatus?.claudeDesktopEnabled ?? false}
            disabled={!activityStatus?.claudeDesktopSupported}
            onChange={(value) => void toggleActivity("activity_claude_desktop_enabled", value)}
          />
          <SettingToggle
            icon={<Globe2 />}
            title="Grok web active minutes"
            detail="Accepts one-minute grok.com active-tab signals from the ArcMeter browser extension over this computer's loopback address."
            checked={activityStatus?.browserBridgeEnabled ?? false}
            onChange={(value) => void toggleActivity("activity_browser_bridge_enabled", value)}
          />
          {activityStatus?.browserBridgeEnabled ? (
            <div className="setting-row bridge-pairing-row">
              <span className="setting-row-icon"><ShieldCheck /></span>
              <div>
                <strong>Browser extension pairing</strong>
                <p>Load <code>extensions/arcmeter-browser-activity</code> as an unpacked Chrome-compatible extension, then paste this local-only token into its Options page. Bridge port: {activityStatus.browserBridgePort}.</p>
                <div className="pairing-token"><input aria-label="Browser pairing token" readOnly value={activityStatus.pairingToken} /><button type="button" className="icon-button" aria-label="Copy browser pairing token" onClick={() => void copyPairingToken()}><Copy /></button></div>
              </div>
            </div>
          ) : null}
          <div className="activity-privacy-note"><ShieldCheck /> Activity-only events contain a source, device, and UTC minute. They never claim token usage or API cost.</div>
        </div>
      </section>

      <section className="settings-section">
        <SettingsHeading title="Subscriptions" description="Track actual recurring cost separately from API-equivalent value." />
        <div className="subscriptions-list">
          {data.subscriptions.map((subscription) => <SubscriptionRow key={subscription.id} subscription={subscription} onSave={onSaveSubscription} />)}
        </div>
      </section>

      <section className="settings-section">
        <SettingsHeading title="Application" description="Background utility behavior on this computer." />
        <div className="settings-panel toggles-panel">
          <div className="setting-row">
            <span className="setting-row-icon"><CloudOff /></span>
            <div><strong>Cross-device sync</strong><p>{auth.signedIn ? `Connected as ${auth.email ?? "ArcMeter user"}. Sync sends normalized metadata only.` : "Sign in to combine this computer with your other ArcMeter devices."}</p></div>
            <button type="button" className="secondary-button" disabled={!auth.signedIn || syncing} onClick={() => void syncCloud()}><RefreshCw className={syncing ? "spin" : ""} />{syncing ? "Syncing" : "Sync now"}</button>
          </div>
          <SettingToggle icon={<RefreshCw />} title="Launch at login" detail="Start ArcMeter quietly after you sign in to this computer." checked={autostart} onChange={(value) => void toggleAutostart(value)} />
          <SettingToggle icon={<HardDrive />} title="Keep running in tray" detail="Closing the dashboard keeps local collection active." checked={closeToTray} onChange={(value) => void toggleCloseToTray(value)} />
          <UpdateSettingRow currentVersion={data.device.appVersion} />
        </div>
      </section>
    </div>
  );
}

function AccountPanel({ auth, onChange }: { auth: AuthStatus; onChange: (status: AuthStatus) => void }) {
  const [email, setEmail] = useState("");
  const [password, setPassword] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  async function submit(event: React.FormEvent) {
    event.preventDefault();
    setBusy(true); setError(null);
    try {
      onChange(await signIn(email, password));
      setPassword("");
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : String(reason));
    } finally { setBusy(false); }
  }
  if (!auth.configured) {
    return (
      <div className="settings-panel account-panel">
        <div className="settings-icon"><CloudOff /></div>
        <div><strong>Cloud account not configured</strong><p>Your local dashboard remains fully available offline. Add the client-safe Supabase URL and publishable key at build time to enable sign-in.</p></div>
        <button type="button" className="secondary-button" disabled>Sign in</button>
      </div>
    );
  }
  if (auth.signedIn) {
    return (
      <div className="settings-panel account-panel">
        <div className="settings-icon"><ShieldCheck /></div>
        <div><strong>{auth.email}</strong><p>Refresh credentials are protected by Windows Credential Manager or macOS Keychain—not SQLite, files, or renderer storage.</p></div>
        <button type="button" className="secondary-button" onClick={() => void signOut().then(onChange)}>Sign out</button>
      </div>
    );
  }
  return (
    <form className="settings-panel account-panel auth-form" onSubmit={(event) => void submit(event)}>
      <div className="settings-icon"><ShieldCheck /></div>
      <div className="auth-fields">
        <input type="email" autoComplete="email" placeholder="Email" value={email} onChange={(event) => setEmail(event.target.value)} required />
        <input type="password" autoComplete="current-password" placeholder="Password" value={password} onChange={(event) => setPassword(event.target.value)} required />
        {error ? <small>{error}</small> : null}
      </div>
      <button type="submit" className="secondary-button" disabled={busy}>{busy ? "Signing in" : "Sign in"}</button>
    </form>
  );
}

function SettingsHeading({ title, description, action }: { title: string; description: string; action?: React.ReactNode }) {
  return <div className="settings-heading"><div><h2>{title}</h2><p>{description}</p></div>{action}</div>;
}

function SubscriptionRow({ subscription, onSave }: { subscription: Subscription; onSave: (subscription: Subscription) => Promise<void> }) {
  const [draft, setDraft] = useState(subscription);
  const [saving, setSaving] = useState(false);
  const changed = JSON.stringify(draft) !== JSON.stringify(subscription);
  async function save() {
    setSaving(true);
    try { await onSave(draft); } finally { setSaving(false); }
  }
  return (
    <div className="subscription-row">
      <div className={`subscription-mark subscription-${subscription.provider}`}><span /></div>
      <div className="subscription-name"><strong>{providerLabel(subscription.provider)}</strong><input aria-label={`${providerLabel(subscription.provider)} plan name`} value={draft.planName} onChange={(event) => setDraft({ ...draft, planName: event.target.value })} /></div>
      <label className="price-input"><span>$</span><input aria-label={`${providerLabel(subscription.provider)} monthly price`} type="number" min="0" step="0.01" value={(draft.monthlyPriceUsdCents / 100).toFixed(2)} onChange={(event) => setDraft({ ...draft, monthlyPriceUsdCents: Math.max(0, Math.round(Number(event.target.value) * 100)) })} /><small>/ month</small></label>
      <label className="switch-label"><span>{draft.active ? "Active" : "Inactive"}</span><input type="checkbox" checked={draft.active} onChange={(event) => setDraft({ ...draft, active: event.target.checked })} /><i /></label>
      <button type="button" className="icon-button" aria-label="Save subscription" disabled={!changed || saving} onClick={() => void save()}><Save /></button>
    </div>
  );
}

function SettingToggle({ icon, title, detail, checked, disabled = false, onChange }: { icon: React.ReactNode; title: string; detail: string; checked: boolean; disabled?: boolean; onChange: (value: boolean) => void }) {
  return (
    <div className="setting-row">
      <span className="setting-row-icon">{icon}</span>
      <div><strong>{title}</strong><p>{detail}</p></div>
      <label className="switch-label compact"><span className="sr-only">{title}</span><input type="checkbox" checked={checked} disabled={disabled} onChange={(event) => onChange(event.target.checked)} /><i /></label>
    </div>
  );
}

function StatusBadge({ status, label }: { status: string; label?: string }) {
  const displayLabel = label ?? (status === "idle" ? "Idle" : `${status[0]?.toUpperCase() ?? ""}${status.slice(1)}`);
  return <span className={`status-badge status-badge-${status}`}><i />{displayLabel}</span>;
}

function ConnectionStatus({ device }: { device: Device }) {
  return <div className={`connection-state connection-${device.syncStatus}`}><span /><div><strong>{device.syncStatus === "local_only" ? "Local only" : device.syncStatus}</strong><small>{device.lastSyncAt ? `Synced ${formatRelativeTime(device.lastSyncAt)}` : "Not yet synced"}</small></div></div>;
}

function osLabel(os: string): string {
  if (os === "windows") return "Windows";
  if (os === "macos") return "macOS";
  return os;
}
