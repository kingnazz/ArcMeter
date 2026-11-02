import { isTauri } from "@tauri-apps/api/core";

// Explicit, development-only visual QA. Vite erases this branch in production.
export const samplePreview = import.meta.env.DEV && !isTauri() && new URLSearchParams(window.location.search).get("preview") === "sample";
export const errorPreview = import.meta.env.DEV && !isTauri() && new URLSearchParams(window.location.search).get("preview") === "error";
export const loadingPreview = import.meta.env.DEV && !isTauri() && new URLSearchParams(window.location.search).get("preview") === "loading";
