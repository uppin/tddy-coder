/**
 * In-memory `the pre-unbundle monolithic RPC coordinate` (+ `auth.AuthService`) backend for ConnectionScreen /
 * SessionsDrawerScreen acceptance tests.
 *
 * `ConnectionService` is daemon-level RPC (`useDaemonClient`, see `../../../src/rpc/selectedDaemon`),
 * routed over the shared common-room LiveKit connection — `mountWithRecordingLiveKitRpc` routes
 * both the HTTP and LiveKit transports to the *same* in-memory backend, so `AuthService`
 * (HTTP, unaffected by daemon selection), `TokenService` (HTTP, per-session/presence LiveKit token
 * issuance — the PRD's bootstrap exception), and `ConnectionService` (LiveKit, daemon-routed) can
 * all be implemented on one backend object here.
 *
 * Fluent-tests preference: an in-memory fake (this file) over wire-level `cy.intercept` — the
 * previous `connectionRpcs.ts` intercept helpers cannot observe LiveKit-transport RPC at all.
 * Field defaults mirror the (still-used-elsewhere) `cy.intercept`-based factories in `./responses.ts`.
 *
 * The host, worktree, terminal and session-file RPCs are no longer this service's: they are
 * `host.HostService`, `worktree.WorktreeService`, `terminal_session.TerminalSessionService` and
 * `session_files.SessionFilesService`, and their fakes live in `./hostServiceBackend`,
 * `./worktreeServiceBackend`, `./terminalSessionServiceBackend` and `./sessionFilesServiceBackend`.
 * This builder composes all five onto one backend so a screen that spans them keeps one scenario
 * object and one set of recorders.
 */

import { create } from "@bufbuild/protobuf";
import { ConnectError, Code } from "@connectrpc/connect";
import { anInMemoryRpcBackend, type InMemoryRpcBackend } from "tddy-connectrpc-testkit";
import { AuthService } from "../../../src/gen/auth_pb";
import { GenerateTokenResponseSchema, RefreshTokenResponseSchema, TokenService } from "../../../src/gen/token_pb";
import {
  ConnectionService,
  ConnectSessionResponseSchema,
  ProjectEntrySchema,
  ResumeSessionResponseSchema,
  SessionEntrySchema,
  StartSessionResponseSchema,
  type ConnectSessionResponse,
  type ProjectEntry,
  type ResumeSessionResponse,
  type SessionEntry,
  type StartSessionResponse,
} from "../../../src/gen/connection_pb";
import {
  AgentInfoSchema,
  CatalogService,
  ToolInfoSchema,
  type AgentInfo,
} from "../../../src/gen/catalog_pb";
import {
  ExecToolService,
  ExecuteToolResponseSchema,
  ToolDefSchema,
} from "../../../src/gen/exec_tools_pb";
import { ActivityService } from "../../../src/gen/activity_pb";
import { HostService } from "../../../src/gen/host_pb";
import { SessionAgentService } from "../../../src/gen/session_agents_pb";
import { SessionFilesService } from "../../../src/gen/session_files_pb";
import { TerminalSessionService } from "../../../src/gen/terminal_session_pb";
import { WorktreeService } from "../../../src/gen/worktree_pb";
import {
  aHostServiceFake,
  DAEMON_LOCAL,
  DAEMON_PEER,
  type DaemonEntry,
  type HostServiceControls,
  type HostServiceScenario,
} from "./hostServiceBackend";
import {
  aSessionFilesServiceFake,
  type SessionFilesServiceControls,
  type SessionFilesServiceScenario,
} from "./sessionFilesServiceBackend";
import {
  aTerminalSessionServiceFake,
  type TerminalSessionServiceControls,
  type TerminalSessionServiceScenario,
} from "./terminalSessionServiceBackend";
import {
  aWorktreeServiceFake,
  type WorktreeServiceControls,
  type WorktreeServiceScenario,
} from "./worktreeServiceBackend";
import {
  aGitHubUser,
  DEFAULT_AGENTS,
  DEFAULT_CLAUDE_CLI_MODEL,
  DEFAULT_CLAUDE_CLI_MODELS,
} from "./responses";
import { acpReplayHandlers, type AcpReplayScenario } from "./acpReplay";
import { aSessionAgentRosterFake, type RosterScenario } from "./sessionAgentRosterBackend";
import {
  anAgentConversationFake,
  type AgentConversationControls,
  type AgentConversationScenario,
} from "./agentConversationBackend";
import { type SessionNotificationFeed } from "./sessionNotificationFeed";

// The daemon-roster fixtures and the worktree row input moved with the RPCs that serve them;
// re-exported here so the specs that already read them off this module keep their import.
export { DAEMON_LOCAL, DAEMON_PEER, type DaemonEntry } from "./hostServiceBackend";
export { type WorktreeStatsRowInput } from "./worktreeServiceBackend";

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

/** The projectId used in collision tests — same project ID across two daemons. */
export const COLLISION_PROJECT_ID = "cccccccc-dddd-4eee-8fff-999999999999";

function aSessionEntry(overrides: Partial<SessionEntry>): SessionEntry {
  return create(SessionEntrySchema, {
    sessionId: "session-default-1",
    createdAt: "2026-03-21T10:00:00Z",
    status: "active",
    repoPath: "/home/dev/project",
    pid: 12345,
    isActive: true,
    projectId: "proj-1",
    daemonInstanceId: "",
    pendingElicitation: false,
    ...overrides,
  });
}

function aProjectEntry(overrides: Partial<ProjectEntry>): ProjectEntry {
  return create(ProjectEntrySchema, {
    projectId: "proj-1",
    name: "Test Project",
    gitUrl: "https://github.com/test/project.git",
    mainRepoPath: "/home/dev/project",
    daemonInstanceId: "",
    ...overrides,
  });
}

function anAgentInfo(overrides: Partial<AgentInfo>): AgentInfo {
  return create(AgentInfoSchema, { id: "claude", label: "Claude (opus)", ...overrides });
}

// ---------------------------------------------------------------------------
// Scenario options
// ---------------------------------------------------------------------------

export interface ConnectionServiceScenario
  extends HostServiceScenario,
    SessionFilesServiceScenario,
    TerminalSessionServiceScenario,
    WorktreeServiceScenario {
  /** Static ListSessions response. Ignored when `listSessionsFactory` is given. */
  sessions?: Partial<SessionEntry>[];
  /** Dynamic ListSessions response, re-evaluated on every call (poll-driven tests). */
  listSessionsFactory?: () => Partial<SessionEntry>[];
  /** ListAgents response. Defaults to `DEFAULT_AGENTS` (the dev.daemon.yaml set). */
  agents?: Array<{ id: string; label: string }>;
  /** ListTools response. Defaults to one `tddy-coder` tool row. */
  tools?: Array<{ path: string; label: string }>;
  /** ListProjects response override (takes precedence over `daemons`-derived defaults). */
  projectsOverride?: Partial<ProjectEntry>[];
  /** ListProjectBranches response. Defaults to empty. */
  projectBranches?: string[];
  /** ConnectSession response. A function derives it per-request (e.g. per sessionId); a plain object is static. */
  connectSession?:
    | Partial<ConnectSessionResponse>
    | ((sessionId: string) => Partial<ConnectSessionResponse>);
  /** ResumeSession response. A function derives it per-request; a plain object is static. */
  resumeSession?:
    | Partial<ResumeSessionResponse>
    | ((sessionId: string) => Partial<ResumeSessionResponse>);
  /** StartSession response. A function derives it per-request; a plain object is static. */
  startSession?:
    | Partial<StartSessionResponse>
    | ((req: { name: string; gitUrl: string }) => Partial<StartSessionResponse>);
  /** When set, SignalSession always fails with this Connect error instead of succeeding. */
  signalSessionError?: { code: Code; message: string };
  /** The session's recorded ACP transcript, served by `StreamAcpReplay` in both modes (and by
   *  `GetAcpToolCallDetail` for the tool bodies). Backs the Agent Activity overlay and the inactive
   *  session's Activities view. Default: unimplemented — omit it unless the spec reads a transcript. */
  acpReplay?: AcpReplayScenario;
  /** The session's agent roster, served by `StreamSessionAgents` (+ attach/detach and the picker's
   *  `ListSubagents`) — what the inspector's Agents tab shows. Default: unimplemented, so a screen
   *  that never opens that tab is unaffected. */
  sessionAgents?: RosterScenario;
  /** Conversations with the session's attached agents, served by `OpenAgentConversation` /
   *  `PromptAgentConversation` / `CancelAgentConversation` — what an agent conversation tab talks
   *  to. Default: unimplemented, so a screen that opens no such tab is unaffected. */
  agentConversations?: AgentConversationScenario;
  /** The daemon-level session-notification feed, served by `StreamSessionNotifications` — one
   *  stream carrying every session's activity and attention events (see `./sessionNotificationFeed`).
   *  Default: unimplemented, so a screen that never subscribes is unaffected. */
  sessionNotifications?: SessionNotificationFeed;
}

export interface ConnectionServiceBackend
  extends InMemoryRpcBackend,
    AgentConversationControls,
    HostServiceControls,
    SessionFilesServiceControls,
    TerminalSessionServiceControls,
    WorktreeServiceControls {
  /** Qualified `agent_id`s passed to `AttachSessionAgent`, in call order. Empty unless the scenario
   *  declared a `sessionAgents` roster. */
  readonly attachedAgentIds: () => string[];
  /** What each `AttachSessionAgent` addressed — the routing half of the write. */
  readonly attachesAddressed: () => { sessionId: string; daemonInstanceId: string }[];
  /** Every `sessionId` passed to `DeleteSession`, in call order. */
  readonly deletedSessionIds: string[];
  /** Every `{ sessionId, signal }` passed to `SignalSession`, in call order. */
  readonly signalCalls: { sessionId: string; signal: number }[];
  /** Every `sessionId` passed to `ExecuteTool`, in call order. */
  readonly executedToolSessionIds: string[];
  /** Every `sessionId` passed to `ConnectSession`, in call order — used by the fast-session-change
   *  regression test to assert re-selecting an already-attached session does NOT re-connect. */
  readonly connectedSessionIds: string[];
}

// ---------------------------------------------------------------------------
// Backend builder
// ---------------------------------------------------------------------------

/**
 * Build an in-memory backend implementing `AuthService.getAuthStatus` (always authenticated) and
 * the `ConnectionService` methods used by `ConnectionScreen`/`SessionsDrawerScreen`.
 */
export function aConnectionServiceBackend(
  scenario: ConnectionServiceScenario = {},
): ConnectionServiceBackend {
  const deletedSessionIds: string[] = [];
  const signalCalls: { sessionId: string; signal: number }[] = [];
  const executedToolSessionIds: string[] = [];
  const connectedSessionIds: string[] = [];

  const defaultDaemons: DaemonEntry[] = [{ instanceId: "local", label: "local (this daemon)", isLocal: true }];
  const daemons = scenario.daemons ?? defaultDaemons;

  const projectsOverride: Partial<ProjectEntry>[] =
    scenario.projectsOverride ??
    (scenario.daemons !== undefined
      ? [{ daemonInstanceId: (daemons.find((d) => d.isLocal) ?? daemons[0])?.instanceId ?? "" }]
      : [{}]);

  // Built once and kept, not inlined into the spread below: these fakes carry the call recorders a
  // spec asserts on, and building them twice would record into a copy nothing can read.
  const rosterFake = scenario.sessionAgents ? aSessionAgentRosterFake(scenario.sessionAgents) : null;
  // The host, worktree, terminal and session-file halves of the scenario, each served by its own
  // service. Built here for the same reason as the fakes above: they carry the recorders a spec
  // asserts on.
  const { handlers: hostHandlers, ...hostControls } = aHostServiceFake(scenario);
  const { handlers: worktreeHandlers, ...worktreeControls } = aWorktreeServiceFake(scenario);
  const { handlers: terminalHandlers, ...terminalControls } = aTerminalSessionServiceFake(scenario);
  const { handlers: sessionFilesHandlers, ...sessionFilesControls } =
    aSessionFilesServiceFake(scenario);
  const conversationFake = scenario.agentConversations
    ? anAgentConversationFake(scenario.agentConversations)
    : null;

  const backend = anInMemoryRpcBackend()
    .implement(HostService, hostHandlers)
    .implement(WorktreeService, worktreeHandlers)
    .implement(TerminalSessionService, terminalHandlers)
    .implement(SessionFilesService, sessionFilesHandlers)
    // `#unbundle` node 7's two coordinates. One `.implement` each, for the same reason the spreads
    // below give: Connect's router fills every omitted method of a registered service with an
    // `Unimplemented` handler, so a second registration of the same service shadows the first.
    .implement(SessionAgentService, {
      // The session's agent roster, and the conversations held with the agents on it.
      ...(rosterFake ? rosterFake.handlers : {}),
      ...(conversationFake ? conversationFake.handlers : {}),
    })
    .implement(ActivityService, {
      // The session's recorded ACP transcript. Spread (rather than re-implemented) so the two-phase
      // replay protocol has one definition shared with `aReplayBackend` — see `./acpReplay`.
      ...(scenario.acpReplay ? acpReplayHandlers(scenario.acpReplay) : {}),
      // The daemon's session-notification feed.
      ...(scenario.sessionNotifications ? scenario.sessionNotifications.handlers : {}),
    })
    .implement(AuthService, {
      getAuthStatus: async () => ({ authenticated: true, user: aGitHubUser() }),
    })
    .implement(TokenService, {
      generateToken: async () =>
        create(GenerateTokenResponseSchema, { token: "mock-jwt-presence", ttlSeconds: 600n }),
      refreshToken: async () =>
        create(RefreshTokenResponseSchema, { token: "mock-jwt-presence", ttlSeconds: 600n }),
    })
    .implement(CatalogService, {
      listTools: async () => ({
        tools: (scenario.tools ?? [{ path: "/usr/bin/tddy-coder", label: "tddy-coder" }]).map((t) =>
          create(ToolInfoSchema, t),
        ),
      }),
      listAgents: async () => ({
        agents: (scenario.agents ?? DEFAULT_AGENTS).map((a) => anAgentInfo(a)),
      }),
      listAgentModels: async () => ({
        models: DEFAULT_CLAUDE_CLI_MODELS,
        defaultModel: DEFAULT_CLAUDE_CLI_MODEL,
      }),
      ...(rosterFake ? rosterFake.catalogHandlers : {}),
    })
    .implement(ExecToolService, {
      listExecTools: async () => ({
        tools: [
          create(ToolDefSchema, {
            name: "Echo",
            description: "Echo a message",
            inputSchemaJson: JSON.stringify({
              type: "object",
              properties: { message: { type: "string" } },
              required: ["message"],
            }),
          }),
        ],
      }),
      listSessionToolCalls: async () => ({ toolCalls: [] }),
      executeTool: async (req) => {
        executedToolSessionIds.push(req.sessionId);
        return create(ExecuteToolResponseSchema, {
          resultJson: '{"ok":true}',
          isError: false,
          errorMessage: "",
        });
      },
    })
    .implement(ConnectionService, {
      listSessions: async () => ({
        sessions: (scenario.listSessionsFactory ? scenario.listSessionsFactory() : (scenario.sessions ?? [])).map(
          (s) => aSessionEntry(s),
        ),
      }),
      listProjects: async () => ({
        projects: projectsOverride.map((p) => aProjectEntry(p)),
      }),
      listProjectBranches: async () => ({ branches: scenario.projectBranches ?? [], defaultRemote: "origin" }),
      connectSession: async (req) => {
        connectedSessionIds.push(req.sessionId);
        const overrides =
          typeof scenario.connectSession === "function"
            ? scenario.connectSession(req.sessionId)
            : scenario.connectSession;
        return create(ConnectSessionResponseSchema, {
          livekitRoom: "session-room-ct",
          livekitUrl: "ws://127.0.0.1:7880",
          livekitServerIdentity: "server",
          ...overrides,
        });
      },
      resumeSession: async (req) => {
        const overrides =
          typeof scenario.resumeSession === "function"
            ? scenario.resumeSession(req.sessionId)
            : scenario.resumeSession;
        return create(ResumeSessionResponseSchema, {
          sessionId: req.sessionId,
          livekitRoom: "resume-room-ct",
          livekitUrl: "ws://127.0.0.1:7880",
          livekitServerIdentity: "server",
          ...overrides,
        });
      },
      startSession: async (req) => {
        const overrides =
          typeof scenario.startSession === "function"
            ? scenario.startSession({ name: req.name, gitUrl: req.gitUrl })
            : scenario.startSession;
        return create(StartSessionResponseSchema, {
          sessionId: "session-started-1",
          livekitRoom: "session-room-ct",
          livekitUrl: "ws://127.0.0.1:7880",
          livekitServerIdentity: "server",
          ...overrides,
        });
      },
      signalSession: async (req) => {
        if (scenario.signalSessionError) {
          throw new ConnectError(scenario.signalSessionError.message, scenario.signalSessionError.code);
        }
        signalCalls.push({ sessionId: req.sessionId, signal: req.signal });
        return {};
      },
      deleteSession: async (req) => {
        deletedSessionIds.push(req.sessionId);
        return {};
      },
    });

  return Object.assign(backend, {
    deletedSessionIds,
    signalCalls,
    executedToolSessionIds,
    connectedSessionIds,
    ...hostControls,
    ...worktreeControls,
    ...terminalControls,
    ...sessionFilesControls,
    // A scenario that declared no roster / no conversations still answers these, with nothing —
    // a spec asserting "never attached" must not have to know whether a fake was built.
    attachedAgentIds: () => (rosterFake ? rosterFake.attachedAgentIds() : []),
    attachesAddressed: () => (rosterFake ? rosterFake.attachesAddressed() : []),
    openedConversations: () => (conversationFake ? conversationFake.openedConversations() : []),
    promptsSent: () => (conversationFake ? conversationFake.promptsSent() : []),
    cancelledConversationIds: () =>
      conversationFake ? conversationFake.cancelledConversationIds() : [],
    releaseAnswer: () => conversationFake?.releaseAnswer(),
    // Throws rather than no-opping when the scenario declared no `agentConversations`: the getters
    // above can honestly answer "nothing happened", but a *setter* that silently discards its setup
    // would leave a spec asserting on a failure that was never armed, and passing.
    failNextPrompt: (message: string) => {
      if (conversationFake === null) {
        throw new Error(
          "failNextPrompt: this backend declared no `agentConversations` scenario, so there is no conversation to fail.",
        );
      }
      conversationFake.failNextPrompt(message);
    },
  });
}

/**
 * `ConnectionServiceScenario` for the "same projectId hosted on two daemons" collision scenario
 * (mirrors the retired `interceptConnectionRpcsProjectIdCollision`).
 */
export function connectionServiceProjectIdCollisionScenario(
  sessions: Partial<SessionEntry>[] = [],
): ConnectionServiceScenario {
  return {
    sessions,
    daemons: [DAEMON_LOCAL, DAEMON_PEER],
    projectsOverride: [
      {
        projectId: COLLISION_PROJECT_ID,
        name: "dup-workstation",
        gitUrl: "https://github.com/test/dup.git",
        mainRepoPath: "/home/ws/dup",
        daemonInstanceId: DAEMON_LOCAL.instanceId,
      },
      {
        projectId: COLLISION_PROJECT_ID,
        name: "dup-server",
        gitUrl: "https://github.com/test/dup.git",
        mainRepoPath: "/srv/dup",
        daemonInstanceId: DAEMON_PEER.instanceId,
      },
    ],
  };
}
