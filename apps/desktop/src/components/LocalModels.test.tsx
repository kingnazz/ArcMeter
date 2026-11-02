import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import type { ActivityItem, LocalModelsSnapshot, LocalRuntime } from "../types";
import { Activity } from "./Activity";
import { LocalModelSettings, LocalModels, LocalOverview } from "./LocalModels";
import { configureLocalRuntime, testLocalRuntime } from "../lib/api";

vi.mock("../lib/api", () => ({ configureLocalRuntime: vi.fn(), testLocalRuntime: vi.fn() }));
afterEach(() => { cleanup(); vi.clearAllMocks(); });
const now = new Date().toISOString();
function runtime(overrides: Partial<LocalRuntime> = {}): LocalRuntime {
  return {provider:"omlx", enabled:true, endpoint:"http://127.0.0.1:8000/v1", automatic:true,
    status:"not_detected",checkedAt:null,detectedAt:null,diagnostic:"Not detected",models:[],...overrides};
}
function local(runtimes: LocalRuntime[] = [runtime()]): LocalModelsSnapshot {
  return {runtimes,measuredTokensToday:0,requestsToday:null,observationsToday:0,historyStatus:"not_found"};
}
describe("Local Models trust boundary", () => {
  it("prevents testing or saving an invalid manual endpoint", () => {
    render(<LocalModelSettings data={local()} onRefresh={vi.fn(() => Promise.resolve())} />);
    fireEvent.click(screen.getByLabelText("oMLX automatic discovery"));
    fireEvent.change(screen.getByLabelText("oMLX endpoint"), {target:{value:"http://remote.example.com:8000"}});
    expect(screen.getByLabelText("oMLX endpoint")).toHaveAttribute("aria-invalid","true");
    expect(screen.getByRole("alert")).toHaveTextContent("HTTP loopback");
    expect(screen.getByRole("button",{name:"Test connection"})).toBeDisabled();
    expect(screen.getByRole("button",{name:"Save"})).toBeDisabled();
    expect(testLocalRuntime).not.toHaveBeenCalled();
    expect(configureLocalRuntime).not.toHaveBeenCalled();
  });
  it("distinguishes an authoritative zero from unavailable token history", () => {
    const data = {...local(),historyStatus:"available",requestsToday:0};
    const view = render(<LocalModels data={data} scanning={false} onScan={vi.fn()} onSettings={vi.fn()} />);
    const tokens = screen.getByText("Local runtime tokens").nextElementSibling;
    expect(tokens).toHaveTextContent("0");
    view.rerender(<LocalModels data={{...data,historyStatus:"not_found",requestsToday:null}} scanning={false} onScan={vi.fn()} onSettings={vi.fn()} />);
    expect(tokens).toHaveTextContent("—");
  });
  it("uses a real empty state with unknown request, session and token measurements", () => {
    render(<LocalModels data={local()} scanning={false} onScan={vi.fn()} onSettings={vi.fn()} />);
    expect(screen.getByText("No loaded model detected")).toBeInTheDocument();
    expect(screen.getByText("Not reported")).toBeInTheDocument();
    expect(screen.getByText(/never estimates tokens or requests/)).toBeInTheDocument();
    expect(screen.getAllByText("Not detected").length).toBeGreaterThan(0);
  });
  it("shows all loaded models, but a stale inventory never appears as running", () => {
    const model = {id:"Qwen",name:"Qwen",loaded:true,contextLength:8192,quantization:null,parameters:null,sizeBytes:null,firstSeen:now,lastSeen:now,lastActivityAt:null};
    const data = local([runtime({status:"healthy",models:[model,{...model,id:"Gemma",name:"Gemma"}]}),runtime({provider:"ollama",status:"unavailable",models:[{...model,id:"Stale",name:"Stale"}]})]);
    render(<LocalOverview local={data} telemetry={{runtimeTokens:100,toolTokens:300,unclassifiedTokens:0,runtimeObservations:0,runtimeTrend:[]}} />);
    expect(screen.getByText("Qwen")).toBeInTheDocument();
    expect(screen.getByText(/2 models loaded/)).toBeInTheDocument();
    expect(screen.getByText("Local runtime telemetry")).toBeInTheDocument();
    expect(screen.getByText("· routing unknown")).toBeInTheDocument();
    expect(screen.getByText("100")).toBeInTheDocument();
    expect(screen.getByText("300")).toBeInTheDocument();
    expect(screen.getByText(/may overlap across layers/)).toBeInTheDocument();
    expect(screen.queryByText(/% local|cloud|workload/i)).not.toBeInTheDocument();
    expect(screen.queryByText("Stale")).not.toBeInTheDocument();
  });
  it("tests draft endpoints without saving and persists manual and disabled choices explicitly", async () => {
    vi.mocked(testLocalRuntime).mockResolvedValue("Connected · 1 models available");
    vi.mocked(configureLocalRuntime).mockResolvedValue();
    const refresh = vi.fn().mockResolvedValue(undefined);
    render(<LocalModelSettings data={local()} onRefresh={refresh} />);
    fireEvent.click(screen.getByLabelText("oMLX automatic discovery"));
    fireEvent.change(screen.getByLabelText("oMLX endpoint"),{target:{value:"http://127.0.0.1:8008/v1"}});
    fireEvent.click(screen.getByRole("button",{name:"Test connection"}));
    await waitFor(() => expect(testLocalRuntime).toHaveBeenCalledWith("omlx","http://127.0.0.1:8008/v1"));
    expect(configureLocalRuntime).not.toHaveBeenCalled();
    await waitFor(() => expect(screen.getByRole("button",{name:"Save"})).toBeEnabled());
    fireEvent.click(screen.getByLabelText("Enable oMLX"));
    fireEvent.click(screen.getByRole("button",{name:"Save"}));
    await waitFor(() => expect(configureLocalRuntime).toHaveBeenCalledWith("omlx",false,"http://127.0.0.1:8008/v1"));
    expect(refresh).toHaveBeenCalledOnce();
  });
  it("keeps runtime observations separate from measured events and foreground minutes", () => {
    const row: ActivityItem = {id:"observation",provider:"omlx",source:"local_model_loaded",occurredAt:now,model:"Qwen",projectName:null,totalTokens:0,inputTokens:0,cachedInputTokens:0,cacheWriteTokens:0,cacheWrite5mTokens:0,cacheWrite1hTokens:0,outputTokens:0,reasoningTokens:0,nativeCostUsdTicks:null,estimatedApiValueUsdMicros:null,measurementKind:"runtime_status",deviceId:"device",deviceName:"This computer"};
    render(<Activity items={[row,{...row,id:"tool",provider:"codex",source:"codex_cli",measurementKind:"measured",totalTokens:120}]} />);
    fireEvent.change(screen.getByLabelText("Telemetry layer"),{target:{value:"runtime"}});
    expect(screen.getByText("oMLX · Model loaded")).toBeInTheDocument();
    expect(screen.queryByRole("button",{name:/Codex/})).not.toBeInTheDocument();
    expect(screen.getByText("1 runtime observations")).toBeInTheDocument();
    expect(screen.queryByText("1 min")).not.toBeInTheDocument();
    fireEvent.click(screen.getByRole("button",{name:/oMLX · Model loaded/}));
    expect(screen.getByText(/No request, token, session or active-time measurement/)).toBeInTheDocument();
  });
  it("shows unavailable oMLX reasoning detail instead of an invented zero", () => {
    const row: ActivityItem = {id:"hourly",provider:"omlx",source:"omlx_usage_hourly",occurredAt:now,model:"Qwen",projectName:null,totalTokens:120,inputTokens:100,cachedInputTokens:30,cacheWriteTokens:0,cacheWrite5mTokens:0,cacheWrite1hTokens:0,outputTokens:20,reasoningTokens:0,nativeCostUsdTicks:null,estimatedApiValueUsdMicros:null,measurementKind:"measured",deviceId:"device",deviceName:"This computer"};
    render(<Activity items={[row]} />);
    fireEvent.click(screen.getByRole("button",{name:/oMLX · hourly usage/}));
    expect(screen.getByText("Not reported · included in output")).toBeInTheDocument();
    expect(screen.getByText("Input (cache included)")).toBeInTheDocument();
    expect(screen.queryByText("Reasoning",{exact:true})).not.toBeInTheDocument();
  });
  it("keeps simultaneous Codex and oMLX rows with the same model and hour separate", () => {
    const row: ActivityItem = {id:"runtime",provider:"omlx",source:"omlx_usage_hourly",occurredAt:now,model:"Qwen",projectName:null,totalTokens:120,inputTokens:100,cachedInputTokens:30,cacheWriteTokens:0,cacheWrite5mTokens:0,cacheWrite1hTokens:0,outputTokens:20,reasoningTokens:0,nativeCostUsdTicks:null,estimatedApiValueUsdMicros:null,measurementKind:"measured",deviceId:"device",deviceName:"This computer"};
    render(<Activity items={[row,{...row,id:"tool",provider:"codex",source:"codex_cli"}]} />);
    expect(screen.getAllByText("120")).toHaveLength(2);
    expect(screen.getByText(/240 telemetry tokens/)).toBeInTheDocument();
    expect(screen.getByText(/Telemetry totals may overlap/)).toBeInTheDocument();
    expect(screen.getByText("Tool · route unknown")).toBeInTheDocument();
    fireEvent.change(screen.getByLabelText("Telemetry layer"),{target:{value:"tool"}});
    expect(screen.queryByRole("button",{name:/oMLX · hourly usage/})).not.toBeInTheDocument();
    expect(screen.getByRole("button",{name:/Codex/})).toBeInTheDocument();
    expect(screen.queryByText(/cloud tools|% local|unique workload/i)).not.toBeInTheDocument();
  });
});
