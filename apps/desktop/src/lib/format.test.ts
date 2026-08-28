import { describe, expect, it } from "vitest";
import { formatTokens, formatUsdCents, formatUsdMicros, providerLabel } from "./format";

describe("formatting", () => {
  it("keeps tokens and currency semantically distinct", () => {
    expect(formatTokens(31_800_000)).toBe("31.8M");
    expect(formatUsdCents(10_000)).toBe("$100");
    expect(formatUsdMicros(1_486_000_000)).toBe("$1,486");
  });

  it("does not invent unavailable monetary value", () => {
    expect(formatUsdMicros(null)).toBe("Unavailable");
  });

  it("uses product-facing provider names", () => {
    expect(providerLabel("claude")).toBe("Claude Code");
  });
});
