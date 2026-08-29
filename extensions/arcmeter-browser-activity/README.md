# ArcMeter Activity Bridge

This unpacked Chrome-compatible extension records privacy-safe active minutes for supported AI web apps. V1 supports `grok.com`.

1. Keep ArcMeter running and enable **Settings → Activity tracking → Grok web active minutes**.
2. Open `chrome://extensions`, enable **Developer mode**, choose **Load unpacked**, and select this directory.
3. Open the extension's **Options**, paste the pairing token from ArcMeter, then choose **Save and test**.

The extension checks the focused tab locally. It sends only `grok_web` and the current UTC minute to `http://127.0.0.1:47639`; it never sends a URL, title, prompt, response, or token count. Repeated samples in the same minute are idempotent.
