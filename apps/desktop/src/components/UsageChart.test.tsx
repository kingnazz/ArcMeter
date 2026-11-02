import { cleanup, fireEvent, render, screen, within } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";
import { UsageChart } from "./UsageChart";
import { endpointError } from "../lib/endpoint";

afterEach(cleanup);
describe("accessible trend exploration", () => {
  it("exposes exact source values and supports arrow, Home and End navigation", () => {
    render(<UsageChart points={[{date:"2026-10-01",label:"Oct 1",tokens:1234},{date:"2026-10-02",label:"Oct 2",tokens:5678},{date:"2026-10-03",label:"Oct 3",tokens:8000}]} />);
    const chart = screen.getByRole("figure", {name:"Telemetry token trend"});
    const first = within(chart).getByRole("button", {name:"Oct 1: 1,234 tokens"});
    expect(first).toHaveAttribute("tabindex","0");
    fireEvent.keyDown(first,{key:"ArrowRight"});
    expect(within(chart).getByRole("button", {name:"Oct 2: 5,678 tokens"})).toHaveFocus();
    expect(chart.querySelector(".chart-readout")).toHaveTextContent("5,678 tokens");
    fireEvent.keyDown(document.activeElement!,{key:"End"});
    expect(within(chart).getByRole("button", {name:"Oct 3: 8,000 tokens"})).toHaveFocus();
    fireEvent.keyDown(document.activeElement!,{key:"Home"});
    expect(first).toHaveFocus();
  });
  it("keeps a single sample explorable and zero-token points truthful", () => {
    const view = render(<UsageChart points={[{date:"2026-10-01",label:"Oct 1",tokens:0}]} />);
    expect(screen.getByRole("button",{name:"Oct 1: 0 tokens"})).toBeInTheDocument();
    expect(screen.getByText("Peak 0")).toBeInTheDocument();
    view.rerender(<UsageChart points={[]} />);
    expect(screen.getByText("No measured trend yet")).toBeInTheDocument();
    expect(screen.queryByRole("button")).not.toBeInTheDocument();
  });
  it("retains a tab stop when a range change shortens the series", () => {
    const view = render(<UsageChart points={[{date:"1",label:"One",tokens:1},{date:"2",label:"Two",tokens:2}]} />);
    fireEvent.keyDown(screen.getByRole("button",{name:"One: 1 tokens"}),{key:"End"});
    view.rerender(<UsageChart points={[{date:"1",label:"One",tokens:1}]} />);
    expect(screen.getByRole("button",{name:"One: 1 tokens"})).toHaveAttribute("tabindex","0");
  });
});
describe("endpoint feedback", () => {
  it("accepts only supported HTTP loopback endpoints before invoking Rust", () => {
    for (const endpoint of ["http://localhost:8000/v1","http://127.0.0.1:11434","http://[::1]:1234/"]) expect(endpointError(endpoint)).toBeNull();
    for (const endpoint of ["https://localhost:8000","http://example.com","http://localhost:8000/private","http://user:secret@localhost:8000","http://localhost:8000?key=secret","not a URL"]) expect(endpointError(endpoint)).not.toBeNull();
  });
});
