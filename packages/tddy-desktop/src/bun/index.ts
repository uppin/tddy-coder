import { join } from "path";

import { BrowserWindow, Utils } from "electrobun/bun";

import { inferOAuthRelayEnvFromDevDesktop } from "./desktop-relay-env";
import {
  registerEmbeddedDaemonCleanup,
  resolveWorkspaceRoot,
  startEmbeddedDaemon,
} from "./embedded-daemon";
import { runLiveKitOAuthRelay } from "./livekit-oauth-relay";
import { waitForDaemonHttp } from "./wait-for-daemon-rpc";

registerEmbeddedDaemonCleanup();
startEmbeddedDaemon();

function mainWindowUrl(): string {
  const vite = process.env.VITE_URL?.trim();
  if (vite) {
    return vite;
  }
  const htmlPath = `${import.meta.dir}/../../resources/web/index.html`;
  return `file://${htmlPath}`;
}

/** Electrobun only forwards `new-window-open` for cmd/ctrl+click unless we add `open-external-links-preload.ts`. */
function extractNewWindowUrl(detail: unknown): string | null {
  if (typeof detail === "string") {
    const t = detail.trim();
    return t.length > 0 ? t : null;
  }
  if (detail && typeof detail === "object" && "url" in detail) {
    const u = (detail as { url: unknown }).url;
    if (typeof u === "string" && u.trim().length > 0) {
      return u;
    }
  }
  return null;
}

function openExternalHttpUrlsFromNewWindow(win: BrowserWindow): void {
  type NavEvt = { data?: { detail?: unknown } };
  const wv = win.webview as unknown as {
    on(name: string, handler: (e: NavEvt) => void): void;
  };
  wv.on("new-window-open", (e: NavEvt) => {
    const url = extractNewWindowUrl(e.data?.detail);
    if (!url) {
      return;
    }
    const lower = url.toLowerCase();
    if (!lower.startsWith("https://") && !lower.startsWith("http://")) {
      return;
    }
    if (!Utils.openExternal(url)) {
      console.warn(
        "[tddy-desktop] Utils.openExternal failed:",
        url.slice(0, 96)
      );
    }
  });
}

const vite = process.env.VITE_URL?.trim();
const openExternalBlankPreload = join(
  import.meta.dir,
  "open-external-links-preload.ts"
);
const mainWindow = new BrowserWindow({
  title: "Tddy Desktop",
  url: mainWindowUrl(),
  sandbox: Boolean(vite && vite.length > 0),
  preload: openExternalBlankPreload,
  frame: { width: 1280, height: 800, x: 120, y: 80 },
});
openExternalHttpUrlsFromNewWindow(mainWindow);

const relayFromYaml = inferOAuthRelayEnvFromDevDesktop(
  resolveWorkspaceRoot(import.meta.dir)
);
const rpc =
  process.env.TDDY_RPC_BASE?.trim() || relayFromYaml?.rpcBase;
const lk =
  process.env.TDDY_LIVEKIT_URL?.trim() ||
  process.env.LIVEKIT_URL?.trim() ||
  process.env.LIVEKIT_PUBLIC_URL?.trim() ||
  relayFromYaml?.livekitUrl;
const room =
  process.env.TDDY_LIVEKIT_ROOM?.trim() || relayFromYaml?.commonRoom;

if (rpc && lk && room) {
  const id =
    process.env.TDDY_DESKTOP_IDENTITY?.trim() ||
    `desktop-${Date.now()}-${Math.random().toString(36).slice(2, 8)}`;
  console.info(
    `[tddy-desktop] Codex OAuth relay scheduled (RPC ${rpc}, room ${room}) — waits for daemon HTTP, then LiveKit; /auth/callback when metadata shows pending`
  );
  void (async () => {
    const httpReady = await waitForDaemonHttp(rpc);
    if (!httpReady) {
      return;
    }
    try {
      await runLiveKitOAuthRelay({
        livekitUrl: lk,
        roomName: room,
        rpcBaseUrl: rpc,
        identity: id,
      });
    } catch (err) {
      console.error("[tddy-desktop] LiveKit OAuth relay failed:", err);
    }
  })();
} else {
  const missing = [
    !rpc && "TDDY_RPC_BASE",
    !lk && "TDDY_LIVEKIT_URL (or LIVEKIT_URL / LIVEKIT_PUBLIC_URL)",
    !room && "TDDY_LIVEKIT_ROOM",
  ].filter(Boolean);
  console.warn(
    `[tddy-desktop] Codex OAuth callback relay disabled (${missing.join(", ")}). ` +
      "Browser redirects to http://localhost:<port>/auth/callback will fail with connection refused. " +
      "Set the env vars or add livekit.url + livekit.common_room to repo-root dev.desktop.yaml."
  );
}
