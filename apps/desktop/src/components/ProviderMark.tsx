import { Braces, Command, Diamond, Sparkles } from "lucide-react";

interface ProviderMarkProps {
  provider: string;
  size?: "small" | "medium";
}
export function ProviderMark({ provider, size = "medium" }: ProviderMarkProps) {
  const Icon = provider === "codex" ? Command : provider === "claude" ? Sparkles : provider === "gemini" ? Diamond : Braces;
  return (
    <span className={`provider-mark provider-${provider} provider-mark-${size}`} aria-hidden="true">
      <Icon strokeWidth={1.8} />
    </span>
  );
}
