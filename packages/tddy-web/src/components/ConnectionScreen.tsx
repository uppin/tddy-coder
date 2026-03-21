import { useEffect, useMemo, useRef, useState } from "react";
import { createClient } from "@connectrpc/connect";
import { createConnectTransport } from "@connectrpc/connect-web";
import {
  ConnectionService,
  type ProjectEntry,
  type SessionEntry,
  type ToolInfo,
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

function sessionsForProject(sessions: SessionEntry[], projectId: string): SessionEntry[] {
  return sessions.filter((s) => s.projectId === projectId);
}

function ConnectedTerminal({
  livekitUrl,
  roomName,
  identity,
  serverIdentity,
  debugLogging,
  onDisconnect,
}: {
  livekitUrl: string;
  roomName: string;
  identity: string;
  serverIdentity: string;
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
        serverIdentity={serverIdentity}
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
  const [projects, setProjects] = useState<ProjectEntry[]>([]);
  const [selectedTool, setSelectedTool] = useState<string>("");
  const [createProjectOpen, setCreateProjectOpen] = useState(false);
  const [newProjectName, setNewProjectName] = useState("");
  const [newProjectGitUrl, setNewProjectGitUrl] = useState("");
  const [newProjectUserRelativePath, setNewProjectUserRelativePath] = useState("");
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [connected, setConnected] = useState<{
    livekitUrl: string;
    roomName: string;
    identity: string;
    serverIdentity: string;
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
    const loadProjects = () => {
      client
        .listProjects({ sessionToken })
        .then((res) => setProjects(res.projects))
        .catch(() => setProjects([]));
    };
    loadSessions();
    loadProjects();
    const interval = setInterval(() => {
      loadSessions();
      loadProjects();
    }, 5000);
    return () => clearInterval(interval);
  }, [client, sessionToken, isAuthenticated]);

  const handleStartSession = async (projectId: string) => {
    if (!sessionToken || !selectedTool || !projectId.trim()) return;
    setError(null);
    try {
      const res = await client.startSession({
        sessionToken,
        toolPath: selectedTool,
        projectId: projectId.trim(),
      });
      setConnected({
        livekitUrl: res.livekitUrl,
        roomName: res.livekitRoom,
        identity: `browser-${res.sessionId}-${Date.now()}`,
        serverIdentity: res.livekitServerIdentity,
        debugLogging,
      });
    } catch (e) {
      setError(e instanceof Error ? e.message : "Failed to start session");
    }
  };

  const handleCreateProject = async () => {
    if (!sessionToken || !newProjectName.trim() || !newProjectGitUrl.trim()) return;
    setError(null);
    try {
      await client.createProject({
        sessionToken,
        name: newProjectName.trim(),
        gitUrl: newProjectGitUrl.trim(),
        userRelativePath: newProjectUserRelativePath.trim(),
      });
      const res = await client.listProjects({ sessionToken });
      setProjects(res.projects);
      setNewProjectName("");
      setNewProjectGitUrl("");
      setNewProjectUserRelativePath("");
      setCreateProjectOpen(false);
    } catch (e) {
      setError(e instanceof Error ? e.message : "Failed to create project");
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
        serverIdentity: res.livekitServerIdentity,
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
        serverIdentity: res.livekitServerIdentity,
        debugLogging,
      });
    } catch (e) {
      setError(e instanceof Error ? e.message : "Failed to resume session");
    }
  };

  const knownProjectIds = useMemo(
    () => new Set(projects.map((p) => p.projectId)),
    [projects]
  );
  const orphanSessions = useMemo(
    () => sessions.filter((s) => !knownProjectIds.has(s.projectId)),
    [sessions, knownProjectIds]
  );

  if (connected) {
    return (
      <ConnectedTerminal
        livekitUrl={connected.livekitUrl}
        roomName={connected.roomName}
        identity={connected.identity}
        serverIdentity={connected.serverIdentity}
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

      <div style={{ marginTop: 16, marginBottom: 8 }}>
        <button
          type="button"
          data-testid="toggle-create-project"
          onClick={() => setCreateProjectOpen((o) => !o)}
          style={{ padding: "6px 12px", fontSize: 14 }}
        >
          {createProjectOpen ? "Hide" : "Create project"}
        </button>
      </div>

      {createProjectOpen && (
        <div
          data-testid="create-project-form"
          style={{
            border: "1px solid #ccc",
            borderRadius: 4,
            padding: 12,
            marginBottom: 16,
          }}
        >
          <label style={labelStyle} htmlFor="new-project-name">
            Project name
          </label>
          <input
            id="new-project-name"
            data-testid="new-project-name"
            type="text"
            placeholder="my-app"
            value={newProjectName}
            onChange={(e) => setNewProjectName(e.target.value)}
            style={inputStyle}
          />
          <label style={labelStyle} htmlFor="new-project-git-url">
            Git URL
          </label>
          <input
            id="new-project-git-url"
            data-testid="new-project-git-url"
            type="text"
            placeholder="https://github.com/org/repo.git"
            value={newProjectGitUrl}
            onChange={(e) => setNewProjectGitUrl(e.target.value)}
            style={inputStyle}
          />
          <label style={labelStyle} htmlFor="new-project-user-relative-path">
            Path under home (optional)
          </label>
          <input
            id="new-project-user-relative-path"
            data-testid="new-project-user-relative-path"
            type="text"
            placeholder="e.g. Code/my-app or ~/Code/my-app — leave empty for default clone path"
            value={newProjectUserRelativePath}
            onChange={(e) => setNewProjectUserRelativePath(e.target.value)}
            style={inputStyle}
          />
          <button
            type="button"
            data-testid="create-project-submit"
            onClick={handleCreateProject}
            disabled={!newProjectName.trim() || !newProjectGitUrl.trim()}
            style={{ padding: "8px 16px" }}
          >
            Create
          </button>
        </div>
      )}

      <label style={{ ...labelStyle, display: "flex", alignItems: "center", gap: 8, marginTop: 8 }}>
        <input
          type="checkbox"
          checked={debugLogging}
          onChange={(e) => setDebugLogging(e.target.checked)}
        />
        Debug logging
      </label>

      {error && (
        <div data-testid="connection-error" style={{ color: "#c00", marginTop: 12 }}>
          {error}
        </div>
      )}

      <h3 style={{ marginTop: 24, fontSize: 16 }}>Projects</h3>
      {projects.length === 0 ? (
        <p style={{ fontSize: 14, color: "#666" }}>No projects yet. Create one above.</p>
      ) : (
        projects.map((p) => (
          <details
            key={p.projectId}
            data-testid={`project-accordion-${p.projectId}`}
            style={{ marginBottom: 12, border: "1px solid #ddd", borderRadius: 4, padding: 8 }}
            open
          >
            <summary style={{ cursor: "pointer", fontWeight: 600 }}>
              {p.name}{" "}
              <span style={{ fontWeight: 400, fontSize: 12, color: "#666" }}> {p.gitUrl}</span>
            </summary>
            <p style={{ fontSize: 12, color: "#555", marginTop: 8 }}>{p.mainRepoPath}</p>
            <button
              type="button"
              data-testid={`start-session-${p.projectId}`}
              onClick={() => handleStartSession(p.projectId)}
              disabled={loading || !selectedTool}
              style={{ marginTop: 8, marginBottom: 8, padding: "8px 16px" }}
            >
              Start New Session
            </button>
            {sessionsForProject(sessions, p.projectId).length === 0 ? (
              <p style={{ fontSize: 14, color: "#666" }}>No sessions for this project.</p>
            ) : (
              <table style={tableStyle} data-testid={`sessions-table-${p.projectId}`}>
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
                  {sessionsForProject(sessions, p.projectId).map((s) => (
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
          </details>
        ))
      )}

      {orphanSessions.length > 0 && (
        <>
          <h3 style={{ marginTop: 24, fontSize: 16 }}>Other sessions</h3>
          <p style={{ fontSize: 13, color: "#666" }}>Sessions not associated with a listed project.</p>
          <table style={tableStyle} data-testid="sessions-table-orphan">
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
              {orphanSessions.map((s) => (
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
        </>
      )}
    </div>
  );
}
