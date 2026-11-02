/** Synthetic visual QA fixtures. Never used by a packaged desktop build. */
import type { ActivityItem, CacheEfficiencyReport, DashboardSnapshot, ProviderQuotaState, RangeKey, SessionDetail, SessionPage, SessionQuery, SessionSummary } from "../types";
const now = new Date();
const stamp = (minutes: number) => new Date(now.getTime() - minutes * 60_000).toISOString();
const models = ["claude-sonnet-4-6", "gpt-5.4", "Qwen3.5-35B-A3B", "gemini-2.5-pro"];
const providers = ["claude", "codex", "omlx", "gemini"];
const sources = ["claude_code", "codex_cli", "omlx_usage_hourly", "gemini_cli"];
export const activity: ActivityItem[] = Array.from({length: 40}, (_, i) => ({
  id: `sample-${i}`, provider: providers[i % 4]!, source: sources[i % 4]!, occurredAt: stamp(i * 9),
  model: models[i % 4]!, projectName: i % 4 === 2 ? null : ["ArcMeter", "api-service", "workspace"][i % 3]!,
  totalTokens: 325_000 - i * 6_000, inputTokens: 240_000 - i * 4_000, cachedInputTokens: 150_000 - i * 3_000,
  cacheWriteTokens: 0, cacheWrite5mTokens: 0, cacheWrite1hTokens: 0, outputTokens: 85_000 - i * 2_000,
  reasoningTokens: 0, nativeCostUsdTicks: null, estimatedApiValueUsdMicros: i % 4 === 2 ? null : 90_000,
  measurementKind: "measured", deviceId: "sample-device", deviceName: "Studio",
}));
export const sessions: SessionSummary[] = Array.from({length: 9}, (_, i) => ({
  sessionKey: `sample-session-${i}`, provider: providers[i % 2]!, source: sources[i % 2]!,
  nativeSessionId: `qa-${i}`, projectName: ["ArcMeter", "api-service", "design-system", "workspace"][i % 4]!,
  startedAt: stamp(i * 75 + 50), lastActivityAt: stamp(i * 75), durationSeconds: 3000,
  eventCount: 42 - i * 3, inputTokens: 500_000 - i * 40_000, cachedInputTokens: 320_000 - i * 25_000,
  cacheWriteTokens: 20_000, cacheWrite5mTokens: 20_000, cacheWrite1hTokens: 0,
  outputTokens: 100_000, reasoningTokens: 0, totalTokens: 940_000 - i * 65_000,
  estimatedApiValueUsdMicros: 3_100_000 - i * 150_000, nativeCostUsdTicks: null, pricingCoverage: "partial",
  primaryModel: models[i % 2]!, modelCount: i === 0 ? 2 : 1, deviceCount: 1, primaryDeviceName: "Studio",
}));
export function sessionPage(query: SessionQuery): SessionPage {
  const matched = sessions.filter((s) => (!query.provider || s.provider === query.provider) && (!query.search || `${s.projectName} ${s.primaryModel} Studio`.toLowerCase().includes(query.search.toLowerCase())));
  const sorted = [...matched].sort((a,b) => query.sort === "tokens" ? b.totalTokens-a.totalTokens : query.sort === "value" ? (b.estimatedApiValueUsdMicros??0)-(a.estimatedApiValueUsdMicros??0) : 0);
  return {sessions: sorted, totalCount: matched.length, stats: {sessionCount: matched.length,totalTokens:matched.reduce((sum,s)=>sum+s.totalTokens,0),estimatedApiValueUsdMicros:matched.reduce((sum,s)=>sum+(s.estimatedApiValueUsdMicros??0),0)},hasMore:false};
}
const cacheSummary: CacheEfficiencyReport["summary"] = {semanticCoverage:"partial",freshInputTokens:4_800_000,cachedInputTokens:18_400_000,cacheWriteTokens:760_000,cacheWrite5mTokens:760_000,cacheWrite1hTokens:0,cacheWriteUnspecifiedTokens:0,normalizedInputContextTokens:24_000_000,reuseShareBps:7667,apiEquivalentCacheImpactUsdMicros:24_800_000,cachePricingCoverage:"partial",measuredEventCount:124};
export function cacheReport(range: CacheEfficiencyReport["range"], provider?: string): CacheEfficiencyReport {
  const weighted = (weight: number): CacheEfficiencyReport["summary"] => ({
    ...cacheSummary, freshInputTokens:Math.round(4_800_000*weight),cachedInputTokens:Math.round(18_400_000*weight),
    cacheWriteTokens:Math.round(760_000*weight),cacheWrite5mTokens:Math.round(760_000*weight),
    normalizedInputContextTokens:Math.round(24_000_000*weight),apiEquivalentCacheImpactUsdMicros:Math.round(24_800_000*weight),measuredEventCount:Math.round(124*weight),
  });
  const weights = [.45,.30,.20,.05];
  const rows = providers.map((p,i)=>({...weighted(weights[i]!),key:p,label:["Claude Code","Codex","oMLX","Gemini CLI"][i]!,provider:p,source:null,model:null,project:null})).filter((row)=>!provider || row.provider===provider);
  const summary = provider ? weighted(weights[providers.indexOf(provider)] ?? 0) : cacheSummary;
  return {range,providerFilter:provider??null,availableProviders:providers,summary,byProvider:rows,byModel:models.slice(0,3).map((model,i)=>({...rows[0]!,...weighted((provider ? weights[providers.indexOf(provider)] ?? 0 : 1)*[.5,.3,.2][i]!),key:model,label:model,model})),byProject:[{...rows[0]!,...summary,key:"ArcMeter",label:"ArcMeter",project:"ArcMeter"}]};
}
export function sessionDetail(session: Pick<SessionSummary,"nativeSessionId">): SessionDetail {
  const s=sessions.find((row)=>row.nativeSessionId===session.nativeSessionId) ?? sessions[0]!;
  return {session:s,models:[{model:s.primaryModel,tokens:s.totalTokens,eventCount:s.eventCount}],devices:["Studio"],cache:cacheSummary,events:activity.slice(0,8).map((a)=>({...a,model:a.model??"Unknown"})),eventsHasMore:false};
}
export function dashboard(range: RangeKey): DashboardSnapshot {
  const breakdown = providers.map((key,i)=>({key,label:["Claude Code CLI","Codex","oMLX","Gemini CLI"][i]!,tokens:[5_120_000,3_040_000,2_080_000,960_000][i]!,percentage:[45.7,27.1,18.6,8.6][i]!}));
  const trend = [280,420,310,650,580,840,530,720,1060,830,690,920,810,1140].map((value,i)=>({date:stamp((13-i)*1440).slice(0,10),label:new Date(stamp((13-i)*1440)).toLocaleDateString("en-US",{month:"short",day:"numeric"}),tokens:value*1000}));
  return {
    generatedAt:stamp(1),range,metrics:{measuredTokensToday:1_140_000,measuredTokensMonth:11_200_000,measuredTokensRange:11_200_000,measuredEventsRange:124,pricedTokensRange:9_120_000,pricedEventsRange:100,activityMinutesRange:186,monthlySubscriptionUsdCents:20000,estimatedApiValueUsdMicros:68_420_000,pricingComplete:false},
    trend,byProvider:breakdown,byModel:breakdown.map((b,i)=>({...b,key:models[i]!,label:models[i]!})),byProject:[{key:"ArcMeter",label:"ArcMeter",tokens:6_400_000,percentage:57.1},{key:"api-service",label:"api-service",tokens:3_200_000,percentage:28.6}],byDevice:[{key:"sample-device",label:"Studio",tokens:11_200_000,percentage:100}],
    telemetry:{runtimeTokens:2_080_000,toolTokens:9_120_000,unclassifiedTokens:0,runtimeObservations:8,runtimeTrend:trend.map((p)=>({...p,tokens:Math.round(p.tokens*.186)}))},
    localModels:{runtimes:providers.slice(2,3).concat(["ollama","lmstudio"]).map((provider,i)=>({provider,enabled:true,automatic:true,endpoint:["http://127.0.0.1:8000/v1","http://127.0.0.1:11434","http://127.0.0.1:1234"][i]!,status:i===2?"not_detected":"healthy",checkedAt:stamp(1),detectedAt:i===2?null:stamp(1),diagnostic:i===2?"No server detected at the automatic endpoint.":"Runtime API reachable. Inventory refreshed.",models:i===2?[]:[{id:i===0?models[2]!:"llama3.3:70b",name:i===0?models[2]!:"llama3.3:70b",loaded:i===0,contextLength:131072,quantization:"Q4_K_M",parameters:i===0?"35B":"70B",sizeBytes:21_400_000_000,firstSeen:stamp(10080),lastSeen:stamp(1),lastActivityAt:i===0?stamp(3):null}]})),measuredTokensToday:420_000,requestsToday:46,observationsToday:8,historyStatus:"available"},
    activity,insights:[{id:"source",title:"Claude is your leading source",detail:"5.12M recorded tokens in the selected period. Source counts may overlap.",tone:"neutral"},{id:"cache",title:"Input reuse is measurable",detail:"Explore provider-specific cache semantics and partial pricing coverage below.",tone:"positive"},{id:"runtime",title:"Local runtime history available",detail:"oMLX reports hourly usage. Loaded inventory does not establish active inference.",tone:"neutral"}],
    sources:breakdown.filter((b)=>b.key!=="omlx").map((b)=>({provider:b.key,label:b.label,detected:true,filesSeen:4,recordsSeen:42,recordsInserted:0,measuredRecords:42,measuredSessions:9,measuredTurns:42,measuredTokens:b.tokens,cacheReadTokens:1_200_000,cacheWriteTokens:0,nativeCostUsdTicks:null,lastScanAt:stamp(1),lastUsageAt:stamp(3),status:"healthy",diagnostics:[]})),
    subscriptions:[{id:"sample-subscription",provider:"anthropic",planName:"Max",monthlyPriceUsdCents:20000,billingCadence:"monthly",active:true,updatedAt:stamp(1)}],
    device:{id:"sample-device",friendlyName:"Studio",os:"macos",architecture:"aarch64",appVersion:"0.5.0",createdAt:stamp(10080),lastSeenAt:stamp(1),lastSyncAt:null,syncStatus:"local_only"},
  };
}
export function quota(): ProviderQuotaState {
  const risk = new URLSearchParams(window.location.search).get("quota") === "risk";
  return {provider:"claude",enabled:true,status:"healthy",message:"Sample account quota.",stale:false,windows:[{key:"five_hour",label:"5-hour",kind:"rolling",scope:null,utilizationBps:6420,periodStartsAt:stamp(180),resetsAt:stamp(-120)}],analyses:[{provider:"claude",windowKey:"five_hour",label:"5-hour",kind:"rolling",capBearing:true,utilizationBps:6420,remainingBps:3580,periodStartsAt:stamp(180),resetsAt:stamp(-120),observedAt:stamp(1),recentBurnBpsPerHour:risk?2400:840,periodAverageBurnBpsPerHour:null,projectedExhaustionAt:stamp(risk?-90:-255),projectedBeforeReset:risk,sampleCount:7,observationSpanSeconds:3600,confidence:"high",status:"active",stale:false}],extraUsage:null,planLabel:"Max",source:"claude_code",observedAt:stamp(1),attemptedAt:stamp(1),retryAt:null,sourceDeviceId:"sample-device",sourceDeviceName:"Studio"};
}
