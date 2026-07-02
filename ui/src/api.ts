// In dev the Vite server (port 5173) proxies /api and /ws to the engine.
// In the packaged Tauri app the UI is served from a custom protocol with no
// proxy, so it must talk to the sidecar engine at its absolute localhost URL.
const dev = location.port === "5173";

export const ENGINE = dev ? "" : "http://127.0.0.1:8787";
export const WS_URL = (dev ? `ws://${location.host}` : "ws://127.0.0.1:8787") + "/ws";

/** Absolute URL for an engine path (e.g. for `<a href>` downloads). */
export const api = (path: string) => `${ENGINE}${path}`;

/** `fetch` against the engine, resolving the base URL automatically. */
export const f = (path: string, init?: RequestInit) => fetch(api(path), init);
