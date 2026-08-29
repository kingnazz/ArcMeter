const BRIDGE_URL = "http://127.0.0.1:47639/v1/activity";

chrome.runtime.onInstalled.addListener(() => {
  chrome.alarms.create("arcmeter-activity-sample", { periodInMinutes: 0.5 });
  void sampleActiveTab();
});

chrome.runtime.onStartup.addListener(() => {
  chrome.alarms.create("arcmeter-activity-sample", { periodInMinutes: 0.5 });
  void sampleActiveTab();
});

chrome.alarms.onAlarm.addListener((alarm) => {
  if (alarm.name === "arcmeter-activity-sample") void sampleActiveTab();
});

chrome.tabs.onActivated.addListener(() => void sampleActiveTab());
chrome.tabs.onUpdated.addListener((_tabId, changeInfo) => {
  if (changeInfo.status === "complete" || changeInfo.url) void sampleActiveTab();
});
chrome.windows.onFocusChanged.addListener((windowId) => {
  if (windowId !== chrome.windows.WINDOW_ID_NONE) void sampleActiveTab();
});

chrome.runtime.onMessage.addListener((message, _sender, sendResponse) => {
  if (message?.type !== "test-bridge") return false;
  sendActivity("grok_web", Math.floor(Date.now() / 60_000))
    .then(() => sendResponse({ ok: true }))
    .catch((error) => sendResponse({ ok: false, error: String(error) }));
  return true;
});

async function sampleActiveTab() {
  const window = await chrome.windows.getLastFocused({ populate: true });
  if (!window.focused) return;
  const tab = window.tabs?.find((candidate) => candidate.active);
  if (!tab?.url || !isGrokUrl(tab.url)) return;
  await sendActivity("grok_web", Math.floor(Date.now() / 60_000));
}

function isGrokUrl(rawUrl) {
  try {
    const url = new URL(rawUrl);
    return url.protocol === "https:" && (url.hostname === "grok.com" || url.hostname.endsWith(".grok.com"));
  } catch {
    return false;
  }
}

async function sendActivity(source, minuteEpoch) {
  const { pairingToken = "" } = await chrome.storage.local.get("pairingToken");
  if (!pairingToken) throw new Error("Open extension options and add the ArcMeter pairing token.");
  const response = await fetch(BRIDGE_URL, {
    method: "POST",
    headers: {
      "Authorization": `Bearer ${pairingToken}`,
      "Content-Type": "application/json"
    },
    body: JSON.stringify({ source, minuteEpoch })
  });
  if (!response.ok) throw new Error(`ArcMeter bridge returned ${response.status}.`);
}
