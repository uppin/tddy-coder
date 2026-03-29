import "./index.css";
import { useEffect, useMemo, useRef, useState, type CSSProperties } from "react";
import { createRoot } from "react-dom/client";
import { createClient } from "@connectrpc/connect";
import { createConnectTransport } from "@connectrpc/connect-web";
import { GhosttyTerminalLiveKit } from "./components/GhosttyTerminalLiveKit";
import { ConnectionTerminalChrome } from "./components/connection/ConnectionTerminalChrome";
import { BUILD_ID } from "./buildId";

function HmrOverlay() {
  const [count, setCount] = useState(0);
  const meta = import.meta as { hot?: { on: (event: string, cb: () => void) => (() => void) | void } };
  const hot = meta.hot;
  useEffect(() => {
    if (!hot) return;
    const dispose = hot.on("vite:afterUpdate", () => setCount((c) => c + 1));
    return () => {
      if (typeof dispose === "function") dispose();
    };
  }, [hot]);
  if (!hot) return null;
  return (
    <span
      data-testid="hmr-count"
      style={{
        position: "fixed",
        bottom: 8,
        left: 8,
        fontSize: 10,
        color: "#888",
        zIndex: 9999,
        fontFamily: "monospace",
      }}
    >
      HMR: {count}
    </span>
  );
}

import { TokenService } from "./gen/token_pb";
import { useAuth } from "./hooks/useAuth";
import { useVisualViewport } from "./hooks/useVisualViewport";
import { GitHubLoginButton } from "./components/GitHubLoginButton";
import { AuthCallback } from "./components/AuthCallback";
import { UserAvatar } from "./components/UserAvatar";
import { Button } from "./components/ui/button";
import { ConnectionScreen } from "./components/ConnectionScreen";

function getParamsFromUrl(): { url: string; identity: string; roomName: string; debugLogging: boolean } {
  const params = typeof window !== "undefined" ? new URLSearchParams(window.location.search) : null;
  return {
    url: params?.get("url") ?? "",
    identity: params?.get("identity") ?? "",
    roomName: params?.get("roomName") ?? "terminal-e2e",
    debugLogging: params?.get("debug") === "1" || params?.get("debugLogging") === "1",
  };
}

function pushParamsToUrl(url: string, identity: string, roomName: string, debugLogging?: boolean): void {
  if (typeof window === "undefined") return;
  const params = new URLSearchParams();
  if (url) params.set("url", url);
  if (identity) params.set("identity", identity);
  if (roomName) params.set("roomName", roomName);
  if (debugLogging) params.set("debug", "1");
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

function createTokenClient() {
  const transport = createConnectTransport({
    baseUrl:
      typeof window !== "undefined"
        ? `${window.location.origin}/rpc`
        : "",
    useBinaryFormat: true,
  });
  return createClient(TokenService, transport);
}

function ConnectedTerminal({
  url,
  identity,
  roomName,
  debugLogging,
  onDisconnect,
  onTerminate,
}: {
  url: string;
  identity: string;
  roomName: string;
  debugLogging?: boolean;
  onDisconnect: () => void;
  /** Standalone GitHub flow has no daemon session — omit Terminate. */
  onTerminate?: () => void;
}) {
  const client = useMemo(() => createTokenClient(), []);
  const fullscreenTargetRef = useRef<HTMLDivElement>(null);
  const [initialToken, setInitialToken] = useState<string | null>(null);
  const [ttlSeconds, setTtlSeconds] = useState<bigint | null>(null);
  const [error, setError] = useState<string | null>(null);
  const { height: viewportHeight, isKeyboardOpen } = useVisualViewport();
  const isMobile =
    typeof window !== "undefined" &&
    (("ontouchstart" in window) || window.innerWidth < 768);

  useEffect(() => {
    client
      .generateToken({ room: roomName, identity })
      .then((res) => {
        setInitialToken(res.token);
        setTtlSeconds(res.ttlSeconds);
      })
      .catch((e) => {
        setError(
          e instanceof Error
            ? e.message
            : "Token fetch failed. Ensure tddy-coder is running with --livekit-api-key and --livekit-api-secret."
        );
      });
  }, [client, roomName, identity]);

  const getToken = useMemo(
    () => async () => {
      const res = await client.refreshToken({ room: roomName, identity });
      return { token: res.token, ttlSeconds: res.ttlSeconds };
    },
    [client, roomName, identity]
  );

  if (error) {
    return (
      <div style={{ padding: 24 }}>
        <div data-testid="livekit-error">{error}</div>
      </div>
    );
  }
  const fullscreenContainerStyle: CSSProperties = {
    position: "fixed",
    top: 0,
    left: 0,
    right: 0,
    height: viewportHeight,
    margin: 0,
    overflow: "hidden",
    display: "flex",
    flexDirection: "column",
  };

  if (!initialToken || ttlSeconds === null) {
    return (
      <div ref={fullscreenTargetRef} data-testid="connected-terminal-container" style={fullscreenContainerStyle}>
        <div style={{ flex: 1, minHeight: 0, position: "relative" }}>
          <ConnectionTerminalChrome
            overlayStatus="connecting"
            buildId={BUILD_ID}
            onDisconnect={onDisconnect}
            onTerminate={onTerminate}
            fullscreenTargetRef={fullscreenTargetRef}
            onStopInterrupt={() => {}}
          />
        </div>
      </div>
    );
  }

  return (
    <div ref={fullscreenTargetRef} data-testid="connected-terminal-container" style={fullscreenContainerStyle}>
      <GhosttyTerminalLiveKit
        url={url}
        token={initialToken}
        getToken={getToken}
        ttlSeconds={ttlSeconds}
        roomName={roomName}
        debugMode={false}
        debugLogging={debugLogging ?? false}
        autoFocus={!isMobile}
        preventFocusOnTap={isMobile && !isKeyboardOpen}
        showMobileKeyboard={isMobile}
        connectionOverlay={{ onDisconnect, buildId: BUILD_ID, onTerminate }}
        fullscreenTargetRef={fullscreenTargetRef}
      />
    </div>
  );
}

function ConnectionForm() {
  const { user, isAuthenticated, login, logout, error: authError } = useAuth();
  const [url, setUrl] = useState("");
  const [identity, setIdentity] = useState("");
  const [roomName, setRoomName] = useState("terminal-e2e");
  const [debugLogging, setDebugLogging] = useState(false);
  const [connected, setConnected] = useState(false);

  useEffect(() => {
    // URL params take priority, then server config, then defaults
    const params = getParamsFromUrl();

    fetch("/api/config")
      .then((r) => (r.ok ? r.json() : null))
      .then((config: { livekit_url?: string; livekit_room?: string } | null) => {
        setUrl(params.url || config?.livekit_url || "");
        setIdentity(params.identity || "");
        setRoomName(params.roomName || config?.livekit_room || "terminal-e2e");
        setDebugLogging(params.debugLogging);
      })
      .catch(() => {
        setUrl(params.url);
        setIdentity(params.identity);
        setRoomName(params.roomName || "terminal-e2e");
        setDebugLogging(params.debugLogging);
      });
  }, []);

  if (connected && url && identity) {
    return (
      <ConnectedTerminal
        url={url}
        identity={identity}
        roomName={roomName}
        debugLogging={debugLogging}
        onDisconnect={() => setConnected(false)}
      />
    );
  }

  if (!isAuthenticated) {
    return (
      <div style={formStyle}>
        <h1>tddy-web</h1>
        <p style={{ marginBottom: 16, fontSize: 14, color: "#444" }}>
          Sign in with GitHub to access the terminal.
        </p>
        {authError ? (
          <p data-testid="auth-flow-error" style={{ marginBottom: 12, fontSize: 14, color: "#c00" }}>
            {authError}
          </p>
        ) : null}
        <GitHubLoginButton onClick={login} />
      </div>
    );
  }

  return (
    <div style={formStyle}>
      <h1>tddy-web</h1>
      {user && <UserAvatar user={user} onLogout={logout} />}
      <form
        onSubmit={(e) => {
          e.preventDefault();
          if (url && identity) {
            pushParamsToUrl(url, identity, roomName, debugLogging);
            setConnected(true);
          }
        }}
      >
        <label style={labelStyle} htmlFor="livekit-url">
          LiveKit URL
        </label>
        <input
          id="livekit-url"
          data-testid="livekit-url"
          type="text"
          placeholder="ws://192.168.1.10:7880"
          value={url}
          onChange={(e) => setUrl(e.target.value)}
          style={inputStyle}
        />
        <label style={labelStyle} htmlFor="livekit-identity">
          Identity
        </label>
        <input
          id="livekit-identity"
          data-testid="livekit-identity"
          type="text"
          placeholder="client"
          value={identity}
          onChange={(e) => setIdentity(e.target.value)}
          style={inputStyle}
        />
        <label style={labelStyle} htmlFor="livekit-room">
          Room name
        </label>
        <input
          id="livekit-room"
          data-testid="livekit-room"
          type="text"
          placeholder="terminal-e2e"
          value={roomName}
          onChange={(e) => setRoomName(e.target.value)}
          style={inputStyle}
        />
        <label style={{ ...labelStyle, display: "flex", alignItems: "center", gap: 8, marginTop: 8 }}>
          <input
            type="checkbox"
            checked={debugLogging}
            onChange={(e) => setDebugLogging(e.target.checked)}
          />
          Debug logging (mouse events, data flow)
        </label>
        <Button type="submit" disabled={!url || !identity}>
          Connect
        </Button>
      </form>
      <p style={{ marginTop: 16, fontSize: 13, color: "#666" }}>
        Token is fetched from the server via Connect-RPC. Ensure tddy-coder is running with
        --livekit-api-key and --livekit-api-secret.
      </p>
    </div>
  );
}

export function App() {
  const path = typeof window !== "undefined" ? window.location.pathname : "/";
  const [appConfig, setAppConfig] = useState<{
    daemonMode: boolean | null;
    livekitUrl?: string;
    commonRoom?: string;
  }>({ daemonMode: null });

  useEffect(() => {
    fetch("/api/config")
      .then((r) => (r.ok ? r.json() : null))
      .then(
        (config: {
          daemon_mode?: boolean;
          livekit_url?: string;
          common_room?: string;
        } | null) => {
          setAppConfig({
            daemonMode: config?.daemon_mode ?? false,
            livekitUrl: config?.livekit_url,
            commonRoom: config?.common_room,
          });
        }
      )
      .catch(() => setAppConfig({ daemonMode: false }));
  }, []);

  const daemonMode = appConfig.daemonMode;

  return (
    <>
      {path === "/auth/callback" ? (
        <AuthCallback />
      ) : daemonMode === null ? (
        <div style={{ padding: 24 }}>Loading…</div>
      ) : daemonMode === true ? (
        <ConnectionScreen livekitUrl={appConfig.livekitUrl} commonRoom={appConfig.commonRoom} />
      ) : (
        <ConnectionForm />
      )}
      <HmrOverlay />
    </>
  );
}

const root = document.getElementById("root");
if (root) {
  createRoot(root).render(<App />);
}
