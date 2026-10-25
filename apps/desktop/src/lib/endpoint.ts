/** Renderer feedback only. Rust remains the authoritative network boundary. */
export function endpointError(value: string): string | null {
  try {
    const url = new URL(value);
    const loopback = url.hostname === "localhost" || url.hostname === "[::1]" || /^127(?:\.\d{1,3}){3}$/.test(url.hostname);
    if (url.protocol === "http:" && loopback && !url.username && !url.password && !url.search && !url.hash && ["", "/v1"].includes(url.pathname.replace(/\/+$/, ""))) return null;
  } catch { /* Show one useful instruction for malformed and unsupported endpoints. */ }
  return "Use an HTTP loopback address, optionally ending in /v1. Credentials and query parameters are not allowed.";
}
