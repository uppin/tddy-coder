import { useEffect, useMemo, useRef, useState } from "react";
import { createClient } from "@connectrpc/connect";
import { createConnectTransport } from "@connectrpc/connect-web";
import {
  ConnectionService,
  type ToolInfo,
  type SessionEntry,
} from "../gen/connection_pb";
import { GhosttyTerminalLiveKit } from "./GhosttyTerminalLiveKit";
import { useAuth } from "../hooks/useAuth";
import { GitHubLoginButton } from "./GitHubLoginButton";
import { UserAvatar } from "./UserAvatar";
import { BUILD_ID } from "../buildId";
import { useVisualViewport } from "../hooks/useVisualViewport";
import { TokenService } from "../gen/token_pb";

const formStyle = {
  padding: 24,
  fontFamily: "system-ui, sans-serif",
  maxWidth: 720,
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

const tableStyle = {
  width: "100%",
  borderCollapse: "collapse" as const,
  marginTop: 12,
  marginBottom: 12,
};

const overlayButtonStyle = {
  position: "absolute" as const,
  top: 8,
  padding: "4px 12px",
  fontSize: 12,
  cursor: "pointer",
  backgroundColor: "rgba(0,0,0,0.6)",
  color: "#ccc",
  border: "1px solid #555",
  borderRadius: 4,
  zIndex: 10,
} as const;

function createConnectionClient() {
  const transport = createConnectTransport({
    baseUrl: typeof window !== "undefined" ? `${window.location.origin}/rpc` : "",
    useBinaryFormat: true,
  });
  return createClient(ConnectionService, transport);
}

function createTokenClient() {
  const transport = createConnectTransport({
    baseUrl: typeof window !== "undefined" ? `${window.location.origin}/rpc` : "",
    useBinaryFormat: true,
  });
  return createClient(TokenService, transport);
}

function truncateId(id: string, maxLen = 12): string {
  if (id.length <= maxLen) return id;
  return `${id.slice(0, 6)}…${id.slice(-4)}`;
}

function ConnectedTerminal({
  livekitUrl,
  roomName,
  identity,
  debugLogging,
  onDisconnect,
}: {
  livekitUrl: string;
  roomName: string;
  identity: string;
  debugLogging?: boolean;
  onDisconnect: () => void;
}) {
  const tokenClient = useMemo(() => createTokenClient(), []);
  const [initialToken, setInitialToken] = useState<string | null>(null);
  const [ttlSeconds, setTtlSeconds] = useState<bigint | null>(null);
  const [error, setError] = useState<string | null>(null);
  const sendCtrlCRef = useRef<(() => void) | null>(null);
  const { height: viewportHeight, isKeyboardOpen } = useVisualViewport();
  const isMobile =
    typeof window !== "undefined" &&
    (("ontouchstart" in window) || window.innerWidth < 768);

  useEffect(() => {
    tokenClient
      .generateToken({ room: roomName, identity })
      .then((res) => {
        setInitialToken(res.token);
        setTtlSeconds(res.ttlSeconds);
      })
      .catch((e) => {
        setError(
          e instanceof Error
            ? e.message
            : "Token fetch failed. Ensure tddy-daemon is running with LiveKit."
        );
      });
  }, [tokenClient, roomName, identity]);

  const getToken = useMemo(
    () => async () => {
      const res = await tokenClient.refreshToken({ room: roomName, identity });
      return { token: res.token, ttlSeconds: res.ttlSeconds };
    },
    [tokenClient, roomName, identity]
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
    <div
      data-testid="connected-terminal-container"
      style={{
        position: "fixed",
        top: 0,
        left: 0,
        right: 0,
        height: viewportHeight,
        margin: 0,
        overflow: "hidden",
        display: "flex",
        flexDirection: "column",
      }}
    >
      <button
        data-testid="ctrl-c-button"
        onClick={() => sendCtrlCRef.current?.()}
        style={{ ...overlayButtonStyle, right: 72 }}
      >
        Ctrl+C
      </button>
      <button
        data-testid="disconnect-button"
        onClick={onDisconnect}
        style={{ ...overlayButtonStyle, right: 8 }}
      >
        Disconnect
      </button>
      <span
        data-testid="build-id"
        style={{
          ...overlayButtonStyle,
          left: 8,
          right: "auto",
          fontSize: 10,
          color: "#888",
          cursor: "default",
        }}
      >
        {BUILD_ID}
      </span>
      <GhosttyTerminalLiveKit
        url={livekitUrl}
        token={initialToken}
        getToken={getToken}
        ttlSeconds={ttlSeconds}
        roomName={roomName}
        debugMode={false}
        debugLogging={debugLogging ?? false}
        autoFocus={!isMobile}
        preventFocusOnTap={isMobile && !isKeyboardOpen}
        showMobileKeyboard={isMobile}
        onRegisterSendCtrlC={(send) => {
          sendCtrlCRef.current = send;
        }}
      />
    </div>
  );
}

export function ConnectionScreen() {
  const { user, isAuthenticated, login, logout, sessionToken } = useAuth();
  const [tools, setTools] = useState<ToolInfo[]>([]);
  const [sessions, setSessions] = useState<SessionEntry[]>([]);
  const [selectedTool, setSelectedTool] = useState<string>("");
  const [repoPath, setRepoPath] = useState("");
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [connected, setConnected] = useState<{
    livekitUrl: string;
    roomName: string;
    identity: string;
    debugLogging: boolean;
  } | null>(null);
  const [debugLogging, setDebugLogging] = useState(false);

  const client = useMemo(() => createConnectionClient(), []);

  useEffect(() => {
    if (!sessionToken || !isAuthenticated) {
      setLoading(false);
      return;
    }
    client
      .listTools({})
      .then((res) => {
        setTools(res.tools);
        setSelectedTool(res.tools[0]?.path ?? "");
      })
      .catch((e) => setError(e instanceof Error ? e.message : "Failed to list tools"))
      .finally(() => setLoading(false));

    const loadSessions = () => {
      client
        .listSessions({ sessionToken })
        .then((res) => setSessions(res.sessions))
        .catch(() => setSessions([]));
    };
    loadSessions();
    const interval = setInterval(loadSessions, 5000);
    return () => clearInterval(interval);
  }, [client, sessionToken, isAuthenticated]);

  const handleStartSession = async () => {
    if (!sessionToken || !selectedTool || !repoPath.trim()) return;
    setError(null);
    try {
      const res = await client.startSession({
        sessionToken,
        toolPath: selectedTool,
        repoPath: repoPath.trim(),
      });
      setConnected({
        livekitUrl: res.livekitUrl,
        roomName: res.livekitRoom,
        identity: `browser-${res.sessionId}-${Date.now()}`,
        debugLogging,
      });
    } catch (e) {
      setError(e instanceof Error ? e.message : "Failed to start session");
    }
  };

  const handleConnectSession = async (sessionId: string) => {
    if (!sessionToken) return;
    setError(null);
    try {
      const res = await client.connectSession({ sessionToken, sessionId });
      setConnected({
        livekitUrl: res.livekitUrl,
        roomName: res.livekitRoom,
        identity: `browser-${sessionId}-${Date.now()}`,
        debugLogging,
      });
    } catch (e) {
      setError(e instanceof Error ? e.message : "Failed to connect to session");
    }
  };

  const handleResumeSession = async (sessionId: string) => {
    if (!sessionToken) return;
    setError(null);
    try {
      const res = await client.resumeSession({ sessionToken, sessionId });
      setConnected({
        livekitUrl: res.livekitUrl,
        roomName: res.livekitRoom,
        identity: `browser-${res.sessionId}-${Date.now()}`,
        debugLogging,
      });
    } catch (e) {
      setError(e instanceof Error ? e.message : "Failed to resume session");
    }
  };

  if (connected) {
    return (
      <ConnectedTerminal
        livekitUrl={connected.livekitUrl}
        roomName={connected.roomName}
        identity={connected.identity}
        debugLogging={connected.debugLogging}
        onDisconnect={() => setConnected(null)}
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
      <h2 style={{ marginTop: 24, fontSize: 18 }}>Start or connect to a session</h2>

      <label style={labelStyle} htmlFor="tool-select">
        Tool
      </label>
      <select
        id="tool-select"
        data-testid="tool-select"
        value={selectedTool}
        onChange={(e) => setSelectedTool(e.target.value)}
        style={{ ...inputStyle, marginBottom: 12 }}
      >
        {tools.map((t) => (
          <option key={t.path} value={t.path}>
            {t.label || t.path}
          </option>
        ))}
      </select>

      <label style={labelStyle} htmlFor="repo-path">
        Repository path
      </label>
      <input
        id="repo-path"
        data-testid="repo-path"
        type="text"
        placeholder="/home/user/project"
        value={repoPath}
        onChange={(e) => setRepoPath(e.target.value)}
        style={inputStyle}
      />

      <label style={{ ...labelStyle, display: "flex", alignItems: "center", gap: 8, marginTop: 8 }}>
        <input
          type="checkbox"
          checked={debugLogging}
          onChange={(e) => setDebugLogging(e.target.checked)}
        />
        Debug logging
      </label>

      <button
        type="button"
        data-testid="start-session"
        onClick={handleStartSession}
        disabled={loading || !selectedTool || !repoPath.trim()}
        style={{ marginTop: 12, marginRight: 8, padding: "8px 16px" }}
      >
        Start New
      </button>

      {error && (
        <div data-testid="connection-error" style={{ color: "#c00", marginTop: 12 }}>
          {error}
        </div>
      )}

      <h3 style={{ marginTop: 24, fontSize: 16 }}>Existing sessions</h3>
      {sessions.length === 0 ? (
        <p style={{ fontSize: 14, color: "#666" }}>No sessions found.</p>
      ) : (
        <table style={tableStyle} data-testid="sessions-table">
          <thead>
            <tr style={{ borderBottom: "1px solid #ccc", textAlign: "left" }}>
              <th style={{ padding: 8 }}>ID</th>
              <th style={{ padding: 8 }}>Date</th>
              <th style={{ padding: 8 }}>Status</th>
              <th style={{ padding: 8 }}>Repo</th>
              <th style={{ padding: 8 }}>PID</th>
              <th style={{ padding: 8 }}>Actions</th>
            </tr>
          </thead>
          <tbody>
            {sessions.map((s) => (
              <tr key={s.sessionId} style={{ borderBottom: "1px solid #eee" }}>
                <td style={{ padding: 8 }}>{truncateId(s.sessionId)}</td>
                <td style={{ padding: 8 }}>{s.createdAt}</td>
                <td style={{ padding: 8 }}>{s.status}</td>
                <td style={{ padding: 8 }}>{s.repoPath}</td>
                <td style={{ padding: 8 }}>{s.pid}</td>
                <td style={{ padding: 8 }}>
                  {s.isActive ? (
                    <button
                      type="button"
                      data-testid={`connect-${s.sessionId}`}
                      onClick={() => handleConnectSession(s.sessionId)}
                      style={{ marginRight: 4, padding: "4px 8px" }}
                    >
                      Connect
                    </button>
                  ) : (
                    <button
                      type="button"
                      data-testid={`resume-${s.sessionId}`}
                      onClick={() => handleResumeSession(s.sessionId)}
                      style={{ padding: "4px 8px" }}
                    >
                      Resume
                    </button>
                  )}
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      )}
    </div>
  );
}
