import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { createClient } from "@connectrpc/connect";
import { createConnectTransport } from "@connectrpc/connect-web";
import {
  ConnectionService,
  Signal,
  type AgentInfo,
  type ProjectEntry,
  type SessionEntry,
  type ToolInfo,
  type EligibleDaemonEntry,
} from "../gen/connection_pb";
import {
  buildAgentSelectOptionsFromRpc,
  coalesceBackendAgentSelection,
} from "./connection/agentOptions";
import { ConnectionSessionTablesSection } from "./connection/ConnectionSessionTablesSection";
import { defaultProjectSessionForm, type ProjectSessionForm } from "./connection/projectSessionForm";
import { GhosttyTerminalLiveKit } from "./GhosttyTerminalLiveKit";
import { ConnectionTerminalChrome } from "./connection/ConnectionTerminalChrome";
import { ParticipantList } from "./ParticipantList";
import { useAuth } from "../hooks/useAuth";
import { useCommonRoom } from "../hooks/useCommonRoom";
import { useRoomParticipants } from "../hooks/useRoomParticipants";
import { GitHubLoginButton } from "./GitHubLoginButton";
import { UserAvatar } from "./UserAvatar";
import { BUILD_ID } from "../buildId";
import { useVisualViewport } from "../hooks/useVisualViewport";
import { TokenService } from "../gen/token_pb";
import { projectForUnscopedSession } from "../utils/sessionProjectTable";
import { SessionWorkflowFilesModal } from "./session/SessionWorkflowFilesModal";
import { Button } from "@/components/ui/button";

/** Full viewport width shell (session tables are not max-width capped). */
const screenShellClassName =
  "min-h-svh w-full min-w-0 box-border px-4 py-6 sm:px-6 font-sans text-foreground";

const inputStyle = {
  display: "block",
  width: "100%",
  marginBottom: 12,
  padding: 8,
  fontSize: 14,
  boxSizing: "border-box" as const,
};

const labelStyle = { display: "block", marginBottom: 4, fontWeight: 500 };

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

function ConnectedTerminal({
  livekitUrl,
  roomName,
  identity,
  serverIdentity,
  debugLogging,
  onDisconnect,
  onTerminate,
}: {
  livekitUrl: string;
  roomName: string;
  identity: string;
  serverIdentity: string;
  debugLogging?: boolean;
  onDisconnect: () => void;
  onTerminate?: () => void | Promise<void>;
}) {
  const tokenClient = useMemo(() => createTokenClient(), []);
  const fullscreenTargetRef = useRef<HTMLDivElement>(null);
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

  const fullscreenContainerStyle = {
    position: "fixed" as const,
    top: 0,
    left: 0,
    right: 0,
    height: viewportHeight,
    margin: 0,
    overflow: "hidden" as const,
    display: "flex" as const,
    flexDirection: "column" as const,
  };

  if (error) {
    return (
      <div style={{ padding: 24 }}>
        <div data-testid="livekit-error">{error}</div>
      </div>
    );
  }
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
          />
        </div>
      </div>
    );
  }

  return (
    <div ref={fullscreenTargetRef} data-testid="connected-terminal-container" style={fullscreenContainerStyle}>
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
        connectionOverlay={{ onDisconnect, buildId: BUILD_ID, onTerminate }}
        fullscreenTargetRef={fullscreenTargetRef}
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
  const [agents, setAgents] = useState<AgentInfo[]>([]);
  const [daemons, setDaemons] = useState<EligibleDaemonEntry[]>([]);
  const [sessions, setSessions] = useState<SessionEntry[]>([]);
  const [projects, setProjects] = useState<ProjectEntry[]>([]);
  const [projectForms, setProjectForms] = useState<Record<string, ProjectSessionForm>>({});
  const [orphanSessionDebug, setOrphanSessionDebug] = useState(false);
  const [workflowFilesSessionId, setWorkflowFilesSessionId] = useState<string | null>(null);
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
    sessionId: string;
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

  const hasActiveSession = useMemo(
    () => sessions.some((s) => s.isActive),
    [sessions]
  );

  const loadSessions = useCallback(() => {
    if (!sessionToken) return;
    client
      .listSessions({ sessionToken })
      .then((res) => setSessions(res.sessions))
      .catch(() => setSessions([]));
  }, [client, sessionToken]);

  useEffect(() => {
    if (!sessionToken || !isAuthenticated) {
      setLoading(false);
      return;
    }
    Promise.all([client.listTools({}), client.listAgents({})])
      .then(([toolsRes, agentsRes]) => {
        setTools(toolsRes.tools);
        setAgents(agentsRes.agents);
      })
      .catch((e) => {
        setTools([]);
        setAgents([]);
        setError(e instanceof Error ? e.message : "Failed to list tools or agents");
      })
      .finally(() => setLoading(false));

    client
      .listEligibleDaemons({ sessionToken })
      .then((res) => setDaemons(res.daemons))
      .catch(() => setDaemons([]));

    const loadProjects = () => {
      client
        .listProjects({ sessionToken })
        .then((res) => setProjects(res.projects))
        .catch(() => setProjects([]));
    };
    loadSessions();
    loadProjects();
    const projectInterval = setInterval(loadProjects, 5000);
    return () => clearInterval(projectInterval);
  }, [client, sessionToken, isAuthenticated, loadSessions]);

  useEffect(() => {
    if (!sessionToken || !isAuthenticated) {
      return;
    }
    const sessionPollMs = hasActiveSession ? 2000 : 5000;
    const sessionInterval = setInterval(loadSessions, sessionPollMs);
    return () => clearInterval(sessionInterval);
  }, [sessionToken, isAuthenticated, hasActiveSession, loadSessions]);

  useEffect(() => {
    setProjectForms((prev) => {
      const next = { ...prev };
      const def = defaultProjectSessionForm(tools, agents, daemons);
      const agentOptions = buildAgentSelectOptionsFromRpc(
        agents.map((a) => ({ id: a.id, label: a.label })),
      );
      for (const p of projects) {
        const existing = next[p.projectId];
        if (!existing) {
          next[p.projectId] = { ...def };
        } else {
          const toolStillValid = tools.some((t) => t.path === existing.toolPath);
          if (!toolStillValid && tools[0]) {
            next[p.projectId] = { ...existing, toolPath: tools[0].path };
          }
          const agentStillValid = agents.some((a) => a.id === existing.agent);
          if (!agentStillValid) {
            next[p.projectId] = {
              ...next[p.projectId],
              agent: coalesceBackendAgentSelection(agentOptions, existing.agent),
            };
          }
          if (!existing.daemonInstanceId && def.daemonInstanceId) {
            next[p.projectId] = { ...next[p.projectId], daemonInstanceId: def.daemonInstanceId };
          }
          if (!existing.recipe?.trim()) {
            next[p.projectId] = { ...next[p.projectId], recipe: def.recipe };
          }
        }
      }
      return next;
    });
  }, [projects, tools, agents, daemons]);

  const updateProjectForm = (projectId: string, patch: Partial<ProjectSessionForm>) => {
    setProjectForms((prev) => ({
      ...prev,
      [projectId]: {
        ...(prev[projectId] ?? defaultProjectSessionForm(tools, agents, daemons)),
        ...patch,
      },
    }));
  };

  const knownProjectIds = useMemo(
    () => new Set(projects.map((p) => p.projectId)),
    [projects]
  );

  const debugForSessionId = (sessionId: string): boolean => {
    const sess = sessions.find((s) => s.sessionId === sessionId);
    if (!sess) return false;
    if (knownProjectIds.has(sess.projectId)) {
      return projectForms[sess.projectId]?.debugLogging ?? false;
    }
    if (sess.projectId.trim() === "") {
      const matched = projectForUnscopedSession(sess, projects);
      if (matched) {
        return projectForms[matched.projectId]?.debugLogging ?? false;
      }
    }
    return orphanSessionDebug;
  };

  const handleStartSession = async (projectId: string) => {
    const form = projectForms[projectId] ?? defaultProjectSessionForm(tools, agents, daemons);
    if (!sessionToken || !form.toolPath || !projectId.trim() || !form.agent) return;
    setError(null);
    try {
      const res = await client.startSession({
        sessionToken,
        toolPath: form.toolPath,
        projectId: projectId.trim(),
        agent: form.agent,
        daemonInstanceId: form.daemonInstanceId,
        recipe: form.recipe,
      });
      setConnected({
        livekitUrl: res.livekitUrl,
        roomName: res.livekitRoom,
        identity: `browser-${res.sessionId}-${Date.now()}`,
        serverIdentity: res.livekitServerIdentity,
        debugLogging: form.debugLogging,
        sessionId: res.sessionId,
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
        sessionId,
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
        sessionId: res.sessionId,
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

  const handleDeleteSession = async (sessionId: string) => {
    if (!sessionToken) return;
    if (
      !window.confirm(
        "Delete this session? If the tool process is still running, it will be stopped first, then on-disk session data will be removed. This cannot be undone."
      )
    ) {
      return;
    }
    setError(null);
    try {
      await client.deleteSession({ sessionToken, sessionId });
      const res = await client.listSessions({ sessionToken });
      setSessions(res.sessions);
    } catch (e) {
      setError(e instanceof Error ? e.message : "Failed to delete session");
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
        onTerminate={() => void handleSignalSession(connected.sessionId, Signal.SIGTERM)}
      />
    );
  }

  if (!isAuthenticated) {
    return (
      <div className={screenShellClassName}>
        <h1>tddy-web</h1>
        <p className="mb-4 text-sm text-muted-foreground">
          Sign in with GitHub to access the terminal.
        </p>
        <GitHubLoginButton onClick={login} />
      </div>
    );
  }

  return (
    <div className={screenShellClassName}>
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

      <div className="my-4">
        <Button
          type="button"
          variant="outline"
          data-testid="toggle-create-project"
          onClick={() => setCreateProjectOpen((o) => !o)}
        >
          {createProjectOpen ? "Hide" : "Create project"}
        </Button>
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
          <Button
            type="button"
            data-testid="create-project-submit"
            onClick={handleCreateProject}
            disabled={!newProjectName.trim() || !newProjectGitUrl.trim()}
          >
            Create
          </Button>
        </div>
      )}

      {error && (
        <div data-testid="connection-error" style={{ color: "#c00", marginTop: 12 }}>
          {error}
        </div>
      )}

      <ConnectionSessionTablesSection
        projects={projects}
        sessions={sessions}
        tools={tools}
        agents={agents}
        daemons={daemons}
        projectForms={projectForms}
        loading={loading}
        orphanSessionDebug={orphanSessionDebug}
        onOrphanSessionDebugChange={setOrphanSessionDebug}
        onUpdateProjectForm={updateProjectForm}
        onStartSession={handleStartSession}
        onConnectSession={handleConnectSession}
        onResumeSession={handleResumeSession}
        onSignalSession={handleSignalSession}
        onDeleteSession={handleDeleteSession}
        onShowWorkflowFiles={setWorkflowFilesSessionId}
      />

      {sessionToken && workflowFilesSessionId ? (
        <SessionWorkflowFilesModal
          open
          onClose={() => setWorkflowFilesSessionId(null)}
          sessionId={workflowFilesSessionId}
          sessionToken={sessionToken}
          client={client}
        />
      ) : null}
    </div>
  );
}
