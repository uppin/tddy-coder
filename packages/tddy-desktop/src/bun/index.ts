import { BrowserWindow } from "electrobun/bun";

import { runLiveKitOAuthRelay } from "./livekit-oauth-relay";

function mainWindowUrl(): string {
  const vite = process.env.VITE_URL?.trim();
  if (vite) {
    return vite;
  }
  const htmlPath = `${import.meta.dir}/../../resources/web/index.html`;
  return `file://${htmlPath}`;
}

const vite = process.env.VITE_URL?.trim();
new BrowserWindow({
  title: "Tddy Desktop",
  url: mainWindowUrl(),
  sandbox: Boolean(vite && vite.length > 0),
  frame: { width: 1280, height: 800, x: 120, y: 80 },
});

const rpc = process.env.TDDY_RPC_BASE?.trim();
const lk = process.env.TDDY_LIVEKIT_URL?.trim();
const room = process.env.TDDY_LIVEKIT_ROOM?.trim();
if (rpc && lk && room) {
  const id =
    process.env.TDDY_DESKTOP_IDENTITY?.trim() ||
    `desktop-${Date.now()}-${Math.random().toString(36).slice(2, 8)}`;
  runLiveKitOAuthRelay({
    livekitUrl: lk,
    roomName: room,
    rpcBaseUrl: rpc,
    identity: id,
  }).catch((err) => {
    console.error("LiveKit OAuth relay failed:", err);
  });
}
