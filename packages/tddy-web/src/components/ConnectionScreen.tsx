import { useCallback, useEffect, useMemo, useRef, useState, type ReactNode } from "react";
import { Trash2 } from "lucide-react";
import { createClient } from "@connectrpc/connect";
import { createConnectTransport } from "@connectrpc/connect-web";
import {
  ConnectionService,
  Signal,
  type ProjectEntry,
  type SessionEntry,
  type ToolInfo,
  type EligibleDaemonEntry,
} from "../gen/connection_pb";
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
import {
  formatSessionCreatedAt,
  sessionIdFirstSegment,
  sessionPidDisplay,
} from "../utils/sessionDisplay";
import { sortSessionsForDisplay } from "../utils/sessionSort";
import { SessionWorkflowStatusCells } from "./SessionWorkflowStatusCells";
import { Button } from "@/components/ui/button";
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/ui/table";

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

type ProjectSessionForm = {
  toolPath: string;
  agent: string;
  /** Workflow recipe: `tdd` or `bugfix` (matches `WorkflowRecipe::name()`). */
  recipe: string;
  debugLogging: boolean;
  daemonInstanceId: string;
};

function defaultProjectSessionForm(tools: ToolInfo[], daemons: EligibleDaemonEntry[]): ProjectSessionForm {
  const localDaemon = daemons.find((d) => d.isLocal);
  return {
    toolPath: tools[0]?.path ?? "",
    agent: "claude",
    recipe: "tdd",
    debugLogging: false,
    daemonInstanceId: localDaemon?.instanceId ?? daemons[0]?.instanceId ?? "",
  };
}

const sessionControlSelectClassName =
  "box-border w-full min-w-[9rem] max-w-[16rem] rounded-md border border-input bg-background px-2 py-1.5 text-sm text-foreground shadow-sm focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring";

/** Tool, backend, host, and browser-terminal debug for one project—per session / connection, not stored on the project. */
function ProjectSessionOptions({
  projectId,
  tools,
  daemons,
  form,
  onChange,
  startSessionButton,
}: {
  projectId: string;
  tools: ToolInfo[];
  daemons: EligibleDaemonEntry[];
  form: ProjectSessionForm;
  onChange: (patch: Partial<ProjectSessionForm>) => void;
  startSessionButton: ReactNode;
}) {
  const toolId = `tool-select-${projectId}`;
  const backendId = `backend-select-${projectId}`;
  const hostId = `host-select-${projectId}`;
  const recipeId = `recipe-select-${projectId}`;
  const debugId = `debug-logging-${projectId}`;
  return (
    <>
      <p className="mb-2 mt-2 text-xs text-muted-foreground">
        Tool, backend, workflow recipe, host, and debug apply only to <strong>Start New Session</strong> and to{" "}
        <strong>Connect / Resume</strong> in this project—not saved on the project.
      </p>
      <div className="flex min-w-0 flex-nowrap items-end gap-3 overflow-x-auto pb-1 pt-1 [scrollbar-width:thin]">
        <div className="flex min-w-[9rem] shrink-0 flex-col gap-1">
          <label className="text-sm font-medium leading-none" htmlFor={hostId}>
            Host (this session)
          </label>
          <select
            id={hostId}
            data-testid={hostId}
            value={form.daemonInstanceId}
            onChange={(e) => onChange({ daemonInstanceId: e.target.value })}
            className={sessionControlSelectClassName}
          >
            {daemons.map((d) => (
              <option key={d.instanceId} value={d.instanceId}>
                {d.label || d.instanceId}
              </option>
            ))}
          </select>
        </div>
        <div className="flex min-w-[9rem] shrink-0 flex-col gap-1">
          <label className="text-sm font-medium leading-none" htmlFor={toolId}>
            Tool (this session)
          </label>
          <select
            id={toolId}
            data-testid={toolId}
            value={form.toolPath}
            onChange={(e) => onChange({ toolPath: e.target.value })}
            className={sessionControlSelectClassName}
          >
            {tools.map((t) => (
              <option key={t.path} value={t.path}>
                {t.label || t.path}
              </option>
            ))}
          </select>
        </div>
        <div className="flex min-w-[9rem] shrink-0 flex-col gap-1">
          <label className="text-sm font-medium leading-none" htmlFor={backendId}>
            Backend (this session)
          </label>
          <select
            id={backendId}
            data-testid={backendId}
            value={form.agent}
            onChange={(e) => onChange({ agent: e.target.value })}
            className={sessionControlSelectClassName}
          >
            <option value="claude">Claude (opus)</option>
            <option value="claude-acp">Claude ACP (opus)</option>
            <option value="cursor">Cursor (composer-2)</option>
            <option value="stub">Stub</option>
          </select>
        </div>
        <div className="flex min-w-[10rem] shrink-0 flex-col gap-1">
          <label className="text-sm font-medium leading-none" htmlFor={recipeId}>
            Workflow recipe (this session)
          </label>
          <select
            id={recipeId}
            data-testid={recipeId}
            value={form.recipe}
            onChange={(e) => onChange({ recipe: e.target.value })}
            className={sessionControlSelectClassName}
          >
            <option value="tdd">TDD (plan → implement)</option>
            <option value="bugfix">Bugfix (reproduce → fix)</option>
          </select>
        </div>
        <label
          className="flex shrink-0 cursor-pointer items-center gap-2 pb-2 text-sm leading-tight"
          htmlFor={debugId}
        >
          <input
            id={debugId}
            data-testid={debugId}
            type="checkbox"
            className="size-4 shrink-0 rounded border border-input accent-primary"
            checked={form.debugLogging}
            onChange={(e) => onChange({ debugLogging: e.target.checked })}
          />
          <span className="max-w-[11rem] sm:max-w-none sm:whitespace-nowrap">
            Debug logging (browser terminal, this connection)
          </span>
        </label>
        <div className="shrink-0 pb-0.5">{startSessionButton}</div>
      </div>
    </>
  );
}

function sortedSessionsForProject(sessions: SessionEntry[], projectId: string): SessionEntry[] {
  return sortSessionsForDisplay(sessions.filter((s) => s.projectId === projectId));
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
    <div ref={ref} className="relative ml-1 inline-block">
      <Button
        type="button"
        variant="outline"
        size="sm"
        data-testid={`signal-dropdown-${sessionId}`}
        onClick={() => setOpen((o) => !o)}
      >
        Signal ▾
      </Button>
      {open && (
        <div
          data-testid={`signal-menu-${sessionId}`}
          className="absolute top-full left-0 z-[1000] min-w-[180px] overflow-hidden rounded-md border border-border bg-popover p-1 text-popover-foreground shadow-md"
        >
          <Button
            type="button"
            variant="ghost"
            size="sm"
            data-testid={`signal-sigint-${sessionId}`}
            className="h-auto w-full justify-start rounded-sm px-3 py-2 font-normal"
            onClick={() => handleClick(Signal.SIGINT)}
          >
            Interrupt (SIGINT)
          </Button>
          <Button
            type="button"
            variant="ghost"
            size="sm"
            data-testid={`signal-sigterm-${sessionId}`}
            className="h-auto w-full justify-start rounded-sm px-3 py-2 font-normal"
            onClick={() => handleClick(Signal.SIGTERM)}
          >
            Terminate (SIGTERM)
          </Button>
          <Button
            type="button"
            variant="ghost"
            size="sm"
            data-testid={`signal-sigkill-${sessionId}`}
            className="h-auto w-full justify-start rounded-sm px-3 py-2 font-normal text-destructive hover:text-destructive"
            onClick={() => handleClick(Signal.SIGKILL)}
          >
            Kill (SIGKILL)
          </Button>
        </div>
      )}
    </div>
  );
}

/** Resume + Delete for inactive session rows (project and orphan tables share stable `data-testid`s). */
function InactiveSessionActions({
  sessionId,
  onResume,
  onDelete,
}: {
  sessionId: string;
  onResume: (sessionId: string) => void;
  onDelete: (sessionId: string) => void | Promise<void>;
}) {
  return (
    <span className="inline-flex flex-wrap items-center gap-2">
      <Button
        type="button"
        variant="secondary"
        size="sm"
        data-testid={`resume-${sessionId}`}
        onClick={() => onResume(sessionId)}
      >
        Resume
      </Button>
      <Button
        type="button"
        variant="destructive"
        size="icon-sm"
        aria-label="Delete session"
        title="Delete session"
        data-testid={`delete-session-${sessionId}`}
        onClick={() => void onDelete(sessionId)}
      >
        <Trash2 />
      </Button>
    </span>
  );
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
            onStopInterrupt={() => {}}
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
  const [daemons, setDaemons] = useState<EligibleDaemonEntry[]>([]);
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
    client
      .listTools({})
      .then((res) => {
        setTools(res.tools);
      })
      .catch((e) => setError(e instanceof Error ? e.message : "Failed to list tools"))
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
      const def = defaultProjectSessionForm(tools, daemons);
      for (const p of projects) {
        const existing = next[p.projectId];
        if (!existing) {
          next[p.projectId] = { ...def };
        } else {
          const toolStillValid = tools.some((t) => t.path === existing.toolPath);
          if (!toolStillValid && tools[0]) {
            next[p.projectId] = { ...existing, toolPath: tools[0].path };
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
  }, [projects, tools, daemons]);

  const updateProjectForm = (projectId: string, patch: Partial<ProjectSessionForm>) => {
    setProjectForms((prev) => ({
      ...prev,
      [projectId]: {
        ...(prev[projectId] ?? defaultProjectSessionForm(tools, daemons)),
        ...patch,
      },
    }));
  };

  const knownProjectIds = useMemo(
    () => new Set(projects.map((p) => p.projectId)),
    [projects]
  );
  const orphanSessions = useMemo(
    () =>
      sortSessionsForDisplay(sessions.filter((s) => !knownProjectIds.has(s.projectId))),
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
    const form = projectForms[projectId] ?? defaultProjectSessionForm(tools, daemons);
    if (!sessionToken || !form.toolPath || !projectId.trim()) return;
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
    if (!window.confirm("Delete this session? This removes on-disk session data and cannot be undone.")) {
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

      <h3 style={{ marginTop: 24, fontSize: 16 }}>Projects</h3>
      {projects.length === 0 ? (
        <p style={{ fontSize: 14, color: "#666" }}>No projects yet. Create one above.</p>
      ) : (
        projects.map((p) => {
          const projectSessions = sortedSessionsForProject(sessions, p.projectId);
          return (
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
                daemons={daemons}
                form={projectForms[p.projectId] ?? defaultProjectSessionForm(tools, daemons)}
                onChange={(patch) => updateProjectForm(p.projectId, patch)}
                startSessionButton={
                  <Button
                    type="button"
                    data-testid={`start-session-${p.projectId}`}
                    onClick={() => handleStartSession(p.projectId)}
                    disabled={
                      loading ||
                      !(projectForms[p.projectId] ?? defaultProjectSessionForm(tools, daemons)).toolPath
                    }
                  >
                    Start New Session
                  </Button>
                }
              />
              {projectSessions.length === 0 ? (
                <p style={{ fontSize: 14, color: "#666" }}>No sessions for this project.</p>
              ) : (
                <Table className="mt-3 w-full min-w-0" data-testid={`sessions-table-${p.projectId}`}>
                  <TableHeader>
                    <TableRow>
                      <TableHead>ID</TableHead>
                      <TableHead>Date</TableHead>
                      <TableHead>Status</TableHead>
                      <TableHead>Host</TableHead>
                      <TableHead>PID</TableHead>
                      <TableHead>Goal</TableHead>
                      <TableHead>Workflow</TableHead>
                      <TableHead>Elapsed</TableHead>
                      <TableHead>Agent</TableHead>
                      <TableHead>Model</TableHead>
                      <TableHead>Actions</TableHead>
                    </TableRow>
                  </TableHeader>
                  <TableBody>
                    {projectSessions.map((s) => (
                      <TableRow key={s.sessionId}>
                        <TableCell>{sessionIdFirstSegment(s.sessionId)}</TableCell>
                        <TableCell>{formatSessionCreatedAt(s.createdAt)}</TableCell>
                        <TableCell>{s.status}</TableCell>
                        <TableCell>{s.daemonInstanceId || "—"}</TableCell>
                        <TableCell>{sessionPidDisplay(s.isActive, s.pid)}</TableCell>
                        <SessionWorkflowStatusCells session={s} />
                        <TableCell>
                          {s.isActive ? (
                            <>
                              <Button
                                type="button"
                                size="sm"
                                data-testid={`connect-${s.sessionId}`}
                                className="mr-1"
                                onClick={() => handleConnectSession(s.sessionId)}
                              >
                                Connect
                              </Button>
                              <SignalDropdown
                                sessionId={s.sessionId}
                                onSignal={handleSignalSession}
                              />
                            </>
                          ) : (
                            <InactiveSessionActions
                              sessionId={s.sessionId}
                              onResume={handleResumeSession}
                              onDelete={handleDeleteSession}
                            />
                          )}
                        </TableCell>
                      </TableRow>
                    ))}
                  </TableBody>
                </Table>
              )}
            </details>
          );
        })
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
          <Table className="mt-3 w-full min-w-0" data-testid="sessions-table-orphan">
            <TableHeader>
              <TableRow>
                <TableHead>ID</TableHead>
                <TableHead>Date</TableHead>
                <TableHead>Status</TableHead>
                <TableHead>Host</TableHead>
                <TableHead>PID</TableHead>
                <TableHead>Goal</TableHead>
                <TableHead>Workflow</TableHead>
                <TableHead>Elapsed</TableHead>
                <TableHead>Agent</TableHead>
                <TableHead>Model</TableHead>
                <TableHead>Actions</TableHead>
              </TableRow>
            </TableHeader>
            <TableBody>
              {orphanSessions.map((s) => (
                <TableRow key={s.sessionId}>
                  <TableCell>{sessionIdFirstSegment(s.sessionId)}</TableCell>
                  <TableCell>{formatSessionCreatedAt(s.createdAt)}</TableCell>
                  <TableCell>{s.status}</TableCell>
                  <TableCell>{s.daemonInstanceId || "—"}</TableCell>
                  <TableCell>{sessionPidDisplay(s.isActive, s.pid)}</TableCell>
                  <SessionWorkflowStatusCells session={s} />
                  <TableCell>
                    {s.isActive ? (
                      <>
                        <Button
                          type="button"
                          size="sm"
                          data-testid={`connect-${s.sessionId}`}
                          className="mr-1"
                          onClick={() => handleConnectSession(s.sessionId)}
                        >
                          Connect
                        </Button>
                        <SignalDropdown
                          sessionId={s.sessionId}
                          onSignal={handleSignalSession}
                        />
                      </>
                    ) : (
                      <InactiveSessionActions
                        sessionId={s.sessionId}
                        onResume={handleResumeSession}
                        onDelete={handleDeleteSession}
                      />
                    )}
                  </TableCell>
                </TableRow>
              ))}
            </TableBody>
          </Table>
        </>
      )}
    </div>
  );
}
