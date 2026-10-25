import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import App from "./App";
import { getDashboard } from "./lib/api";
import { dashboard } from "./dev/fixtures";

vi.mock("./lib/api", async (importOriginal) => ({
  ...await importOriginal<Record<string, unknown>>(),
  getDashboard: vi.fn(),
}));
afterEach(() => { cleanup(); vi.mocked(getDashboard).mockReset(); });

describe("workspace loading and recovery", () => {
  it("keeps a labeled loading skeleton while the ledger is pending", () => {
    vi.mocked(getDashboard).mockReturnValue(new Promise(() => {}));
    render(<App />);
    expect(screen.getByLabelText("Loading ArcMeter")).toBeInTheDocument();
    expect(screen.queryByText("11.2M")).not.toBeInTheDocument();
  });
  it("explains an unavailable ledger and recovers on Retry", async () => {
    vi.mocked(getDashboard).mockRejectedValueOnce(new Error("Ledger temporarily unavailable")).mockResolvedValue(dashboard("today"));
    render(<App />);
    expect(await screen.findByRole("alert")).toHaveTextContent("Unable to load your ledger");
    expect(screen.getByRole("alert")).toHaveTextContent("Ledger temporarily unavailable");
    fireEvent.click(screen.getByRole("button", {name:"Retry"}));
    expect(await screen.findByRole("heading", {name:"Telemetry trend"})).toBeInTheDocument();
    expect(screen.queryByRole("alert")).not.toBeInTheDocument();
  });
});
