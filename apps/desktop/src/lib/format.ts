const compactNumber = new Intl.NumberFormat("en-US", {
  notation: "compact",
  maximumFractionDigits: 1,
});

const currency = new Intl.NumberFormat("en-US", {
  style: "currency",
  currency: "USD",
  maximumFractionDigits: 0,
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

export function formatRelativeTime(value: string | null): string {
  if (!value) return "Never";
  const deltaSeconds = Math.round((Date.now() - new Date(value).getTime()) / 1000);
  if (Math.abs(deltaSeconds) < 60) return "Just now";
  const formatter = new Intl.RelativeTimeFormat("en", { numeric: "auto" });
  if (Math.abs(deltaSeconds) < 3_600) return formatter.format(-Math.round(deltaSeconds / 60), "minute");
  if (Math.abs(deltaSeconds) < 86_400) return formatter.format(-Math.round(deltaSeconds / 3_600), "hour");
  return formatter.format(-Math.round(deltaSeconds / 86_400), "day");
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
