import { useEffect, useMemo, useState } from "react";
import { createRoot } from "react-dom/client";
import { createClient } from "@connectrpc/connect";
import { createConnectTransport } from "@connectrpc/connect-web";
import { GhosttyTerminalLiveKit } from "./components/GhosttyTerminalLiveKit";
import { TokenService } from "./gen/token_pb";
import { useAuth } from "./hooks/useAuth";
import { GitHubLoginButton } from "./components/GitHubLoginButton";
import { AuthCallback } from "./components/AuthCallback";
import { UserAvatar } from "./components/UserAvatar";

function getParamsFromUrl(): { url: string; identity: string; roomName: string } {
  const params = typeof window !== "undefined" ? new URLSearchParams(window.location.search) : null;
  return {
    url: params?.get("url") ?? "",
    identity: params?.get("identity") ?? "",
    roomName: params?.get("roomName") ?? "terminal-e2e",
  };
}

function pushParamsToUrl(url: string, identity: string, roomName: string): void {
  if (typeof window === "undefined") return;
  const params = new URLSearchParams();
  if (url) params.set("url", url);
  if (identity) params.set("identity", identity);
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
}: {
  url: string;
  identity: string;
  roomName: string;
}) {
  const client = useMemo(() => createTokenClient(), []);
  const [initialToken, setInitialToken] = useState<string | null>(null);
  const [ttlSeconds, setTtlSeconds] = useState<bigint | null>(null);
  const [error, setError] = useState<string | null>(null);

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
  if (!initialToken || ttlSeconds === null) {
    return (
      <div style={{ padding: 24 }}>
        <div data-testid="livekit-status">connecting</div>
      </div>
    );
  }

  return (
    <div style={{ height: "100vh", display: "flex", flexDirection: "column" }}>
      <GhosttyTerminalLiveKit
        url={url}
        token={initialToken}
        getToken={getToken}
        ttlSeconds={ttlSeconds}
        roomName={roomName}
        debugMode={false}
      />
    </div>
  );
}

function ConnectionForm() {
  const { user, isAuthenticated, login, logout } = useAuth();
  const [url, setUrl] = useState("");
  const [identity, setIdentity] = useState("");
  const [roomName, setRoomName] = useState("terminal-e2e");
  const [connected, setConnected] = useState(false);

  useEffect(() => {
    const { url: u, identity: i, roomName: r } = getParamsFromUrl();
    setUrl(u);
    setIdentity(i);
    setRoomName(r);
  }, []);

  if (connected && url && identity) {
    return (
      <ConnectedTerminal
        url={url}
        identity={identity}
        roomName={roomName}
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
            pushParamsToUrl(url, identity, roomName);
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
        <button type="submit" disabled={!url || !identity}>
          Connect
        </button>
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

  if (path === "/auth/callback") {
    return <AuthCallback />;
  }

  return <ConnectionForm />;
}

const root = document.getElementById("root");
if (root) {
  createRoot(root).render(<App />);
}
