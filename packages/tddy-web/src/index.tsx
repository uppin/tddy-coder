import { useEffect, useState } from "react";
import { createRoot } from "react-dom/client";
import { GhosttyTerminalLiveKit } from "./components/GhosttyTerminalLiveKit";

function getParamsFromUrl(): { url: string; token: string; roomName: string } {
  const params = typeof window !== "undefined" ? new URLSearchParams(window.location.search) : null;
  return {
    url: params?.get("url") ?? "",
    token: params?.get("token") ?? "",
    roomName: params?.get("roomName") ?? "terminal-e2e",
  };
}

function pushParamsToUrl(url: string, token: string, roomName: string): void {
  if (typeof window === "undefined") return;
  const params = new URLSearchParams();
  if (url) params.set("url", url);
  if (token) params.set("token", token);
  if (roomName) params.set("roomName", roomName);
  const search = params.toString();
  const newUrl = search ? `${window.location.pathname}?${search}` : window.location.pathname;
  window.history.replaceState(null, "", newUrl);
}

const formStyle = {
  padding: 24,
  fontFamily: "system-ui, sans-serif",
  maxWidth: 560,
} as const;

const inputStyle = {
  display: "block",
  width: "100%",
  marginBottom: 12,
  padding: 8,
  fontSize: 14,
  boxSizing: "border-box" as const,
};

const labelStyle = { display: "block", marginBottom: 4, fontWeight: 500 };

function App() {
  const [url, setUrl] = useState("");
  const [token, setToken] = useState("");
  const [roomName, setRoomName] = useState("terminal-e2e");
  const [connected, setConnected] = useState(false);

  useEffect(() => {
    const { url: u, token: t, roomName: r } = getParamsFromUrl();
    setUrl(u);
    setToken(t);
    setRoomName(r);
  }, []);

  if (connected && url && token) {
    return (
      <div style={{ height: "100vh", display: "flex", flexDirection: "column" }}>
        <GhosttyTerminalLiveKit url={url} token={token} roomName={roomName} debugMode={true} />
      </div>
    );
  }

  return (
    <div style={formStyle}>
      <h1>tddy-web</h1>
      <form
        onSubmit={(e) => {
          e.preventDefault();
          if (url && token) {
            pushParamsToUrl(url, token, roomName);
            setConnected(true);
          }
        }}
      >
        <label style={labelStyle} htmlFor="livekit-url">
          LiveKit URL
        </label>
        <input
          id="livekit-url"
          type="text"
          placeholder="ws://192.168.1.10:7880"
          value={url}
          onChange={(e) => setUrl(e.target.value)}
          style={inputStyle}
        />
        <label style={labelStyle} htmlFor="livekit-token">
          Token
        </label>
        <input
          id="livekit-token"
          type="password"
          placeholder="JWT access token"
          value={token}
          onChange={(e) => setToken(e.target.value)}
          style={inputStyle}
        />
        <label style={labelStyle} htmlFor="livekit-room">
          Room name
        </label>
        <input
          id="livekit-room"
          type="text"
          placeholder="terminal-e2e"
          value={roomName}
          onChange={(e) => setRoomName(e.target.value)}
          style={inputStyle}
        />
        <button type="submit" disabled={!url || !token}>
          Connect
        </button>
      </form>
      <p style={{ marginTop: 16, fontSize: 13, color: "#666" }}>
        Generate a client token:{" "}
        <code>lk token create --api-key devkey --api-secret secret --room {roomName || "terminal-e2e"} --identity client --join</code>
      </p>
    </div>
  );
}

const root = document.getElementById("root");
if (root) {
  createRoot(root).render(<App />);
}
