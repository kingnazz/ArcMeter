import { Braces, Command, Diamond, Sparkles, Cpu } from "lucide-react";
import { isLocalProvider } from "../lib/format";

interface ProviderMarkProps {
  provider: string;
  size?: "small" | "medium";
}
export function ProviderMark({ provider, size = "medium" }: ProviderMarkProps) {
  const Icon = isLocalProvider(provider) ? Cpu : provider === "codex" ? Command : provider === "claude" ? Sparkles : provider === "gemini" ? Diamond : Braces;
  return (
    <span className={`provider-mark provider-${provider} provider-mark-${size}`} aria-hidden="true">
      <Icon strokeWidth={1.8} />
    </span>
  );
}
