import { useEffect, useMemo, useRef, useState } from "react";
import { createClient } from "@connectrpc/connect";
import { createConnectTransport } from "@connectrpc/connect-web";
import {
  ConnectionService,
  Signal,
  type ProjectEntry,
  type SessionEntry,
  type ToolInfo,
} from "../gen/connection_pb";
import { GhosttyTerminalLiveKit } from "./GhosttyTerminalLiveKit";
import { ParticipantList } from "./ParticipantList";
import { useAuth } from "../hooks/useAuth";
import { useCommonRoom } from "../hooks/useCommonRoom";
import { useRoomParticipants } from "../hooks/useRoomParticipants";
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

type ProjectSessionForm = {
  toolPath: string;
  agent: string;
  debugLogging: boolean;
};

function defaultProjectSessionForm(tools: ToolInfo[]): ProjectSessionForm {
  return {
    toolPath: tools[0]?.path ?? "",
    agent: "claude",
    debugLogging: false,
  };
}

/** Tool, backend, and browser-terminal debug for one project—per session / connection, not stored on the project. */
function ProjectSessionOptions({
  projectId,
  tools,
  form,
  onChange,
}: {
  projectId: string;
  tools: ToolInfo[];
  form: ProjectSessionForm;
  onChange: (patch: Partial<ProjectSessionForm>) => void;
}) {
  const toolId = `tool-select-${projectId}`;
  const backendId = `backend-select-${projectId}`;
  const debugId = `debug-logging-${projectId}`;
  return (
    <>
      <p style={{ fontSize: 12, color: "#666", marginTop: 8, marginBottom: 8 }}>
        Tool, backend, and debug apply only to <strong>Start New Session</strong> and to{" "}
        <strong>Connect / Resume</strong> in this project—not saved on the project.
      </p>
      <label style={labelStyle} htmlFor={toolId}>
        Tool (this session)
      </label>
      <select
        id={toolId}
        data-testid={toolId}
        value={form.toolPath}
        onChange={(e) => onChange({ toolPath: e.target.value })}
        style={{ ...inputStyle, marginBottom: 12 }}
      >
        {tools.map((t) => (
          <option key={t.path} value={t.path}>
            {t.label || t.path}
          </option>
        ))}
      </select>
      <label style={labelStyle} htmlFor={backendId}>
        Backend (this session)
      </label>
      <select
        id={backendId}
        data-testid={backendId}
        value={form.agent}
        onChange={(e) => onChange({ agent: e.target.value })}
        style={{ ...inputStyle, marginBottom: 12 }}
      >
        <option value="claude">Claude (opus)</option>
        <option value="claude-acp">Claude ACP (opus)</option>
        <option value="cursor">Cursor (composer-2)</option>
        <option value="stub">Stub</option>
      </select>
      <label
        style={{ ...labelStyle, display: "flex", alignItems: "center", gap: 8, marginTop: 4 }}
        htmlFor={debugId}
      >
        <input
          id={debugId}
          data-testid={debugId}
          type="checkbox"
          checked={form.debugLogging}
          onChange={(e) => onChange({ debugLogging: e.target.checked })}
        />
        Debug logging (browser terminal, this connection)
      </label>
    </>
  );
}

function sessionsForProject(sessions: SessionEntry[], projectId: string): SessionEntry[] {
  return sessions.filter((s) => s.projectId === projectId);
}

function SignalDropdown({
  sessionId,
  onSignal,
}: {
  sessionId: string;
  onSignal: (sessionId: string, signal: Signal) => void;
}) {
  const [open, setOpen] = useState(false);
  const ref = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!open) return;
    const handler = (e: MouseEvent) => {
      if (ref.current && !ref.current.contains(e.target as Node)) {
        setOpen(false);
      }
    };
    document.addEventListener("mousedown", handler);
    return () => document.removeEventListener("mousedown", handler);
  }, [open]);

  const handleClick = (signal: Signal) => {
    setOpen(false);
    onSignal(sessionId, signal);
  };

  return (
    <div ref={ref} style={{ display: "inline-block", position: "relative", marginLeft: 4 }}>
      <button
        type="button"
        data-testid={`signal-dropdown-${sessionId}`}
        onClick={() => setOpen((o) => !o)}
        style={{ padding: "4px 8px" }}
      >
        Signal ▾
      </button>
      {open && (
        <div
          data-testid={`signal-menu-${sessionId}`}
          style={{
            position: "absolute",
            top: "100%",
            left: 0,
            background: "#fff",
            border: "1px solid #ccc",
            borderRadius: 4,
            boxShadow: "0 2px 8px rgba(0,0,0,0.12)",
            zIndex: 10,
            minWidth: 180,
          }}
        >
          <button
            type="button"
            data-testid={`signal-sigint-${sessionId}`}
            onClick={() => handleClick(Signal.SIGINT)}
            style={{ display: "block", width: "100%", textAlign: "left", padding: "8px 12px", border: "none", background: "none", cursor: "pointer" }}
          >
            Interrupt (SIGINT)
          </button>
          <button
            type="button"
            data-testid={`signal-sigterm-${sessionId}`}
            onClick={() => handleClick(Signal.SIGTERM)}
            style={{ display: "block", width: "100%", textAlign: "left", padding: "8px 12px", border: "none", background: "none", cursor: "pointer" }}
          >
            Terminate (SIGTERM)
          </button>
          <button
            type="button"
            data-testid={`signal-sigkill-${sessionId}`}
            onClick={() => handleClick(Signal.SIGKILL)}
            style={{ display: "block", width: "100%", textAlign: "left", padding: "8px 12px", border: "none", background: "none", cursor: "pointer" }}
          >
            Kill (SIGKILL)
          </button>
        </div>
      )}
    </div>
  );
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
        connectionOverlay={{ onDisconnect, buildId: BUILD_ID }}
      />
    </div>
  );
}

export function ConnectionScreen({
  livekitUrl,
  commonRoom,
}: {
  livekitUrl?: string;
  commonRoom?: string;
} = {}) {
  const { user, isAuthenticated, isLoading, login, logout, sessionToken } = useAuth();
  const [tools, setTools] = useState<ToolInfo[]>([]);
  const [sessions, setSessions] = useState<SessionEntry[]>([]);
  const [projects, setProjects] = useState<ProjectEntry[]>([]);
  const [projectForms, setProjectForms] = useState<Record<string, ProjectSessionForm>>({});
  const [orphanSessionDebug, setOrphanSessionDebug] = useState(false);
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
  const client = useMemo(() => createConnectionClient(), []);

  const presenceReady =
    Boolean(commonRoom?.trim() && livekitUrl?.trim()) &&
    isAuthenticated &&
    !isLoading &&
    Boolean(user);

  const presenceIdentity = user ? `web-${user.login}` : undefined;

  const { room: presenceRoom, status: presenceStatus, error: presenceError } = useCommonRoom(
    presenceReady ? livekitUrl : undefined,
    presenceReady ? commonRoom : undefined,
    presenceReady ? presenceIdentity : undefined
  );

  const participants = useRoomParticipants(presenceReady ? presenceRoom : null);

  useEffect(() => {
    if (!sessionToken || !isAuthenticated) {
      setLoading(false);
      return;
    }
    client
      .listTools({})
      .then((res) => {
        setTools(res.tools);
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

  useEffect(() => {
    setProjectForms((prev) => {
      const next = { ...prev };
      const def = defaultProjectSessionForm(tools);
      for (const p of projects) {
        const existing = next[p.projectId];
        if (!existing) {
          next[p.projectId] = { ...def };
        } else {
          const toolStillValid = tools.some((t) => t.path === existing.toolPath);
          if (!toolStillValid && tools[0]) {
            next[p.projectId] = { ...existing, toolPath: tools[0].path };
          }
        }
      }
      return next;
    });
  }, [projects, tools]);

  const updateProjectForm = (projectId: string, patch: Partial<ProjectSessionForm>) => {
    setProjectForms((prev) => ({
      ...prev,
      [projectId]: {
        ...(prev[projectId] ?? defaultProjectSessionForm(tools)),
        ...patch,
      },
    }));
  };

  const knownProjectIds = useMemo(
    () => new Set(projects.map((p) => p.projectId)),
    [projects]
  );
  const orphanSessions = useMemo(
    () => sessions.filter((s) => !knownProjectIds.has(s.projectId)),
    [sessions, knownProjectIds]
  );

  const debugForSessionId = (sessionId: string): boolean => {
    const sess = sessions.find((s) => s.sessionId === sessionId);
    if (!sess) return false;
    if (knownProjectIds.has(sess.projectId)) {
      return projectForms[sess.projectId]?.debugLogging ?? false;
    }
    return orphanSessionDebug;
  };

  const handleStartSession = async (projectId: string) => {
    const form = projectForms[projectId] ?? defaultProjectSessionForm(tools);
    if (!sessionToken || !form.toolPath || !projectId.trim()) return;
    setError(null);
    try {
      const res = await client.startSession({
        sessionToken,
        toolPath: form.toolPath,
        projectId: projectId.trim(),
        agent: form.agent,
      });
      setConnected({
        livekitUrl: res.livekitUrl,
        roomName: res.livekitRoom,
        identity: `browser-${res.sessionId}-${Date.now()}`,
        serverIdentity: res.livekitServerIdentity,
        debugLogging: form.debugLogging,
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
        debugLogging: debugForSessionId(sessionId),
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
        debugLogging: debugForSessionId(sessionId),
      });
    } catch (e) {
      setError(e instanceof Error ? e.message : "Failed to resume session");
    }
  };

  const handleSignalSession = async (sessionId: string, signal: Signal) => {
    if (!sessionToken) return;
    setError(null);
    try {
      await client.signalSession({ sessionToken, sessionId, signal });
    } catch (e) {
      setError(e instanceof Error ? e.message : "Failed to send signal");
    }
  };

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

      {presenceReady && (
        <div
          data-testid="connected-participants-panel"
          style={{
            marginTop: 16,
            marginBottom: 16,
            border: "1px solid #ddd",
            borderRadius: 4,
            padding: 12,
          }}
        >
          <h3 style={{ marginTop: 0, fontSize: 16 }}>Connected participants</h3>
          <ParticipantList
            participants={participants}
            roomStatus={presenceStatus}
            connectionError={presenceError}
          />
        </div>
      )}

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
            <ProjectSessionOptions
              projectId={p.projectId}
              tools={tools}
              form={projectForms[p.projectId] ?? defaultProjectSessionForm(tools)}
              onChange={(patch) => updateProjectForm(p.projectId, patch)}
            />
            <button
              type="button"
              data-testid={`start-session-${p.projectId}`}
              onClick={() => handleStartSession(p.projectId)}
              disabled={
                loading ||
                !(projectForms[p.projectId] ?? defaultProjectSessionForm(tools)).toolPath
              }
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
                          <>
                            <button
                              type="button"
                              data-testid={`connect-${s.sessionId}`}
                              onClick={() => handleConnectSession(s.sessionId)}
                              style={{ marginRight: 4, padding: "4px 8px" }}
                            >
                              Connect
                            </button>
                            <SignalDropdown
                              sessionId={s.sessionId}
                              onSignal={handleSignalSession}
                            />
                          </>
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
          <label
            style={{ ...labelStyle, display: "flex", alignItems: "center", gap: 8, marginTop: 8 }}
            htmlFor="orphan-session-debug"
          >
            <input
              id="orphan-session-debug"
              data-testid="orphan-session-debug"
              type="checkbox"
              checked={orphanSessionDebug}
              onChange={(e) => setOrphanSessionDebug(e.target.checked)}
            />
            Debug logging (browser terminal, Connect / Resume below)
          </label>
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
                      <>
                        <button
                          type="button"
                          data-testid={`connect-${s.sessionId}`}
                          onClick={() => handleConnectSession(s.sessionId)}
                          style={{ marginRight: 4, padding: "4px 8px" }}
                        >
                          Connect
                        </button>
                        <SignalDropdown
                          sessionId={s.sessionId}
                          onSignal={handleSignalSession}
                        />
                      </>
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
