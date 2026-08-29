const tokenInput = document.querySelector("#token");
const status = document.querySelector("#status");

chrome.storage.local.get("pairingToken").then(({ pairingToken = "" }) => {
  tokenInput.value = pairingToken;
});

document.querySelector("#save").addEventListener("click", async () => {
  const pairingToken = tokenInput.value.trim();
  status.textContent = "Testing…";
  status.className = "";
  if (!pairingToken) {
    status.textContent = "Enter a pairing token.";
    status.className = "error";
    return;
  }
  await chrome.storage.local.set({ pairingToken });
  const result = await chrome.runtime.sendMessage({ type: "test-bridge" });
  status.textContent = result?.ok ? "Connected to ArcMeter." : result?.error ?? "Could not connect.";
  status.className = result?.ok ? "success" : "error";
});
