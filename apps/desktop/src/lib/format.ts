const compactNumber = new Intl.NumberFormat("en-US", {
  notation: "compact",
  maximumFractionDigits: 1,
});

const currency = new Intl.NumberFormat("en-US", {
  style: "currency",
  currency: "USD",
  maximumFractionDigits: 0,
});

const preciseCurrency = new Intl.NumberFormat("en-US", {
  style: "currency",
  currency: "USD",
  minimumFractionDigits: 2,
  maximumFractionDigits: 4,
});

export function formatTokens(value: number): string {
  if (!Number.isFinite(value)) return "—";
  return compactNumber.format(Math.max(0, value)).replace("B", "B").replace("M", "M");
}
export function formatTokenDetail(value: number): string {
  return new Intl.NumberFormat("en-US").format(Math.max(0, value));
}

export function formatMinutes(value: number): string {
  const minutes = Math.max(0, Math.round(value));
  if (minutes < 60) return `${minutes.toLocaleString()} min`;
  const hours = Math.floor(minutes / 60);
  const remainder = minutes % 60;
  return remainder === 0 ? `${hours.toLocaleString()} hr` : `${hours.toLocaleString()} hr ${remainder} min`;
}

export function activitySourceLabel(source: string, provider: string): string {
  if (source === "claude_desktop") return "Claude Desktop";
  if (source === "grok_web") return "Grok web";
  return providerLabel(provider);
}

export function formatUsdCents(value: number): string {
  return currency.format(value / 100);
}

export function formatUsdMicros(value: number | null): string {
  if (value === null) return "Unavailable";
  return currency.format(value / 1_000_000);
}

export function formatUsdTicks(value: number | null): string {
  if (value === null) return "Unavailable";
  return preciseCurrency.format(value / 10_000_000_000);
}

export function formatRelativeTime(value: string | null): string {
  if (!value) return "Never";
  const deltaSeconds = Math.round((Date.now() - new Date(value).getTime()) / 1000);
  if (Math.abs(deltaSeconds) < 60) return "Just now";
  const formatter = new Intl.RelativeTimeFormat("en", { numeric: "auto" });
  if (Math.abs(deltaSeconds) < 3_600) return formatter.format(-Math.round(deltaSeconds / 60), "minute");
  if (Math.abs(deltaSeconds) < 86_400) return formatter.format(-Math.round(deltaSeconds / 3_600), "hour");
  return formatter.format(-Math.round(deltaSeconds / 86_400), "day");
}

export function formatQuotaPercent(basisPoints: number): string {
  const percent = Math.min(10_000, Math.max(0, basisPoints)) / 100;
  return `${Number.isInteger(percent) ? percent.toFixed(0) : percent.toFixed(2).replace(/0$/, "")}%`;
}

export function formatQuotaReset(value: string | null, now = Date.now()): string {
  if (!value) return "Reset time unavailable";
  const reset = new Date(value);
  const remaining = reset.getTime() - now;
  if (!Number.isFinite(reset.getTime())) return "Reset time unavailable";
  if (remaining <= 0) return "Reset pending";
  if (remaining <= 24 * 60 * 60 * 1000) {
    const totalMinutes = Math.max(1, Math.ceil(remaining / 60_000));
    const hours = Math.floor(totalMinutes / 60);
    const minutes = totalMinutes % 60;
    return hours > 0 ? `Resets in ${hours}h ${minutes}m` : `Resets in ${minutes}m`;
  }
  return `Resets ${new Intl.DateTimeFormat("en-US", { weekday: "short", hour: "numeric", minute: "2-digit" }).format(reset)}`;
}

export function formatQuotaPace(basisPointsPerHour: number): string {
  const points = Math.max(0, basisPointsPerHour) / 100;
  const value = points >= 10 ? points.toFixed(0) : points.toFixed(1).replace(/\.0$/, "");
  return `+${value} pts/hr`;
}

export function formatProjectedDuration(value: string, now = Date.now()): string {
  const milliseconds = new Date(value).getTime() - now;
  if (!Number.isFinite(milliseconds) || milliseconds <= 0) return "now";
  const totalMinutes = Math.max(1, Math.round(milliseconds / 60_000));
  const hours = Math.floor(totalMinutes / 60);
  const minutes = totalMinutes % 60;
  if (hours === 0) return `${minutes}m`;
  return minutes === 0 ? `${hours}h` : `${hours}h ${minutes}m`;
}

export function formatMinorCurrency(value: number | null, code: string | null): string {
  if (value === null) return "Unavailable";
  const currencyCode = code && /^[A-Z]{3}$/.test(code) ? code : "USD";
  return new Intl.NumberFormat("en-US", { style: "currency", currency: currencyCode }).format(value / 100);
}

export function formatActivityTime(value: string): string {
  return new Intl.DateTimeFormat("en-US", { hour: "numeric", minute: "2-digit" }).format(new Date(value));
}

export function formatActivityDate(value: string): string {
  const date = new Date(value);
  const today = new Date();
  const yesterday = new Date(today);
  yesterday.setDate(today.getDate() - 1);
  if (date.toDateString() === today.toDateString()) return "Today";
  if (date.toDateString() === yesterday.toDateString()) return "Yesterday";
  return new Intl.DateTimeFormat("en-US", { month: "short", day: "numeric", year: date.getFullYear() === today.getFullYear() ? undefined : "numeric" }).format(date);
}

export function formatSessionDuration(seconds: number): string {
  const minutes = Math.floor(Math.max(0, seconds) / 60);
  if (minutes < 1) return "<1m";
  const hours = Math.floor(minutes / 60);
  const remainder = minutes % 60;
  return hours === 0 ? `${minutes}m` : remainder === 0 ? `${hours}h` : `${hours}h ${remainder}m`;
}

export function formatUsdMicrosPrecise(value: number | null): string {
  if (value === null) return "Pricing unavailable";
  return preciseCurrency.format(value / 1_000_000);
}

export function providerLabel(provider: string): string {
  const labels: Record<string, string> = {
    codex: "Codex",
    claude: "Claude Code CLI",
    grok: "Grok Build",
    gemini: "Gemini CLI",
    openai: "ChatGPT Personal",
    openai_business: "ChatGPT Business",
    anthropic: "Claude",
    xai: "Grok",
    google: "Google AI / Gemini",
  };
  return labels[provider] ?? provider;
}
