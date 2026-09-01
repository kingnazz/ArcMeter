import { describe, expect, it } from "vitest";
import { formatQuotaPercent, formatQuotaReset, formatTokens, formatUsdCents, formatUsdMicros, providerLabel } from "./format";

describe("formatting", () => {
  it("keeps tokens and currency semantically distinct", () => {
    expect(formatTokens(31_800_000)).toBe("31.8M");
    expect(formatUsdCents(10_000)).toBe("$100");
    expect(formatUsdMicros(1_486_000_000)).toBe("$1,486");
  });

  it("preserves quota precision and chooses countdown versus local date", () => {
    expect(formatQuotaPercent(4_763)).toBe("47.63%");
    expect(formatQuotaPercent(4_760)).toBe("47.6%");
    expect(formatQuotaReset("2026-08-31T14:14:00Z", Date.parse("2026-08-31T12:00:00Z"))).toBe("Resets in 2h 14m");
    expect(formatQuotaReset(null)).toBe("Reset time unavailable");
  });

  it("does not invent unavailable monetary value", () => {
    expect(formatUsdMicros(null)).toBe("Unavailable");
  });

  it("uses product-facing provider names", () => {
    expect(providerLabel("claude")).toBe("Claude Code CLI");
  });
});
