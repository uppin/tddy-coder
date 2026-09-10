/**
 * In-memory `connection.ConnectionService` (+ `auth.AuthService`) backend for ConnectionScreen /
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
 * The host and worktree RPCs are no longer this service's: they are `host.HostService` and
 * `worktree.WorktreeService`, and their fakes live in `./hostServiceBackend` and
 * `./worktreeServiceBackend`. This builder composes all three onto one backend so a screen that
 * spans them keeps one scenario object and one set of recorders.
 */

import { create } from "@bufbuild/protobuf";
import { ConnectError, Code } from "@connectrpc/connect";
import { anInMemoryRpcBackend, type InMemoryRpcBackend } from "tddy-connectrpc-testkit";
import { AuthService } from "../../../src/gen/auth_pb";
import { GenerateTokenResponseSchema, RefreshTokenResponseSchema, TokenService } from "../../../src/gen/token_pb";
import {
  AgentInfoSchema,
  ConnectionService,
  ConnectSessionResponseSchema,
  ProjectEntrySchema,
  ResumeSessionResponseSchema,
  SessionEntrySchema,
  StartSessionResponseSchema,
  ToolInfoSchema,
  ClaimTerminalControlResponseSchema,
  ExecuteToolResponseSchema,
  ListExecToolsResponseSchema,
  ListSessionToolCallsResponseSchema,
  ListTerminalSessionsResponseSchema,
  SessionTerminalOutputSchema,
  StartTerminalSessionResponseSchema,
  StopTerminalSessionResponseSchema,
  TerminalHistoryChunkSchema,
  TerminalSessionInfoSchema,
  ToolDefSchema,
  type AgentInfo,
  type ConnectSessionResponse,
  type ProjectEntry,
  type ResumeSessionResponse,
  type SessionEntry,
  type StartSessionResponse,
} from "../../../src/gen/connection_pb";
import { HostService } from "../../../src/gen/host_pb";
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

export interface ConnectionServiceScenario extends HostServiceScenario, WorktreeServiceScenario {
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
  /** Initial `ListTerminalSessions` result — the bash terminals already open on the session.
   *  Defaults to none (only the reserved "main"/Agent terminal, which is not listed here). */
  terminals?: Array<{ terminalId: string; kind?: string; pid?: number }>;
  /** The `terminal_id` handed out by the Nth (0-based) `StartTerminalSession`. Default `bash-<n+1>`. */
  newTerminalId?: (index: number) => string;
  /** Absolute `endOffset` carried by the initial `StreamTerminalOutput` replay frame — the anchor
   *  for lazy scroll-up history. When set (> 0n), the backend's `streamTerminalOutput` emits its
   *  identifying frame tagged with this offset (and `atOldest`), mirroring the daemon's lazy
   *  replay. Default: 0n (no offset metadata — legacy shape). */
  terminalReplayEndOffset?: bigint;
  /** How many `StreamTerminalOutput` subscriptions **end** after their first frame instead of
   *  staying open — the daemon dropping a terminal feed (a closed data-channel stream, a `pty_done`).
   *  The first `n` opened streams drop; every later one stays open, so a test can drop a feed once
   *  and still observe what the screen does to recover. Default 0 (no stream ever ends). */
  droppedTerminalStreams?: number;
  /** Older history chunks returned by `GetTerminalHistory`, in the order they should be yielded
   *  across successive calls (one chunk per call). Each chunk's `startOffset`/`endOffset`/`atOldest`
   *  is used verbatim. The backend pops one chunk per `getTerminalHistory` call. */
  terminalHistory?: Array<{ data: Uint8Array; startOffset: bigint; endOffset: bigint; atOldest: boolean }>;
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
  /** Every `sessionId` passed to `ClaimTerminalControl`, in call order. */
  readonly claimedControlSessionIds: string[];
  /** Every `sessionId` passed to `ConnectSession`, in call order — used by the fast-session-change
   *  regression test to assert re-selecting an already-attached session does NOT re-connect. */
  readonly connectedSessionIds: string[];
  /** Every `sessionId` passed to `StartTerminalSession`, in call order. */
  readonly startTerminalSessionIds: string[];
  /** The `terminal_id` handed back by each `StartTerminalSession`, in call order. */
  readonly startedTerminalIds: string[];
  /** Every `{ sessionId, terminalId }` passed to `StopTerminalSession`, in call order. */
  readonly stoppedTerminals: { sessionId: string; terminalId: string }[];
  /** Every `{ sessionId, terminalId, data }` passed to `SendTerminalInput`, in call order. */
  readonly sentTerminalInput: { sessionId: string; terminalId: string; data: Uint8Array }[];
  /** Every `{ sessionId, terminalId }` an output stream was opened for, in call order. */
  readonly streamedTerminals: { sessionId: string; terminalId: string }[];
  /** Every `{ sessionId, terminalId, beforeOffset }` passed to `GetTerminalHistory`, in call order. */
  readonly getTerminalHistoryCalls: { sessionId: string; terminalId: string; beforeOffset: bigint }[];
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
  const claimedControlSessionIds: string[] = [];
  const connectedSessionIds: string[] = [];
  const startTerminalSessionIds: string[] = [];
  const startedTerminalIds: string[] = [];
  const stoppedTerminals: { sessionId: string; terminalId: string }[] = [];
  const sentTerminalInput: { sessionId: string; terminalId: string; data: Uint8Array }[] = [];
  const streamedTerminals: { sessionId: string; terminalId: string }[] = [];
  const getTerminalHistoryCalls: { sessionId: string; terminalId: string; beforeOffset: bigint }[] = [];

  // Live bash-terminal list — mutated by Start/Stop so ListTerminalSessions stays consistent.
  const liveTerminals: { terminalId: string; kind: string; pid: number }[] = (
    scenario.terminals ?? []
  ).map((t, i) => ({ terminalId: t.terminalId, kind: t.kind ?? "bash", pid: t.pid ?? 8000 + i }));
  const nextTerminalId = scenario.newTerminalId ?? ((index: number) => `bash-${index + 1}`);

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
  // The host and worktree halves of the scenario, each served by its own service. Built here for
  // the same reason as the fakes above: they carry the recorders a spec asserts on.
  const { handlers: hostHandlers, ...hostControls } = aHostServiceFake(scenario);
  const { handlers: worktreeHandlers, ...worktreeControls } = aWorktreeServiceFake(scenario);
  const conversationFake = scenario.agentConversations
    ? anAgentConversationFake(scenario.agentConversations)
    : null;

  const backend = anInMemoryRpcBackend()
    .implement(HostService, hostHandlers)
    .implement(WorktreeService, worktreeHandlers)
    .implement(AuthService, {
      getAuthStatus: async () => ({ authenticated: true, user: aGitHubUser() }),
    })
    .implement(TokenService, {
      generateToken: async () =>
        create(GenerateTokenResponseSchema, { token: "mock-jwt-presence", ttlSeconds: 600n }),
      refreshToken: async () =>
        create(RefreshTokenResponseSchema, { token: "mock-jwt-presence", ttlSeconds: 600n }),
    })
    .implement(ConnectionService, {
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
      // The session's recorded ACP transcript. Spread (rather than re-implemented) so the two-phase
      // replay protocol has one definition shared with `aReplayBackend` — see `./acpReplay`.
      ...(scenario.acpReplay ? acpReplayHandlers(scenario.acpReplay) : {}),
      // The session's agent roster. Spread for the same reason as the replay handlers: one
      // `.implement(ConnectionService, …)` per backend, since Connect's router fills every omitted
      // method of a registered service with an `Unimplemented` handler.
      ...(rosterFake ? rosterFake.handlers : {}),
      // The agent conversations behind the session's agent tabs, spread for the same reason.
      ...(conversationFake ? conversationFake.handlers : {}),
      // The daemon's session-notification feed. Spread for the same reason as the two above: one
      // `.implement(ConnectionService, …)` per backend.
      ...(scenario.sessionNotifications ? scenario.sessionNotifications.handlers : {}),
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
      claimTerminalControl: async (req) => {
        claimedControlSessionIds.push(req.sessionId);
        return create(ClaimTerminalControlResponseSchema, { granted: true, controlToken: "ctrl-1" });
      },
      // Server-streaming control watch — yield nothing in tests.
      watchTerminalControl: async function* () {
        yield { $typeName: "connection.TerminalControlEvent", event: { case: "granted", value: "ctrl-1" } } as any;
      },
      // --- Multiple terminals per session ---
      listTerminalSessions: async () =>
        create(ListTerminalSessionsResponseSchema, {
          terminals: liveTerminals.map((t) => create(TerminalSessionInfoSchema, t)),
        }),
      startTerminalSession: async (req) => {
        const terminalId = nextTerminalId(startTerminalSessionIds.length);
        startTerminalSessionIds.push(req.sessionId);
        startedTerminalIds.push(terminalId);
        liveTerminals.push({ terminalId, kind: "bash", pid: 8000 + liveTerminals.length });
        return create(StartTerminalSessionResponseSchema, { terminalId });
      },
      stopTerminalSession: async (req) => {
        stoppedTerminals.push({ sessionId: req.sessionId, terminalId: req.terminalId });
        const at = liveTerminals.findIndex((t) => t.terminalId === req.terminalId);
        if (at !== -1) liveTerminals.splice(at, 1);
        return create(StopTerminalSessionResponseSchema, { ok: true, message: "" });
      },
      sendTerminalInput: async (req) => {
        sentTerminalInput.push({
          sessionId: req.sessionId,
          terminalId: req.terminalId,
          data: req.data,
        });
        return {};
      },
      // Server-streaming output — record the opened stream, emit one identifying frame, then stay
      // open (a terminal stream that *completes* would signal disconnect and evict the runtime).
      // When `scenario.terminalReplayEndOffset` is set, the frame is tagged with the absolute
      // `endOffset` + `atOldest` so the lazy scroll-up loader can anchor older-history fetches.
      // The frame carries the session and RESOLVED terminal id it came from, as the daemon stamps
      // every frame — a pane drops frames that are not its own.
      // `scenario.droppedTerminalStreams` makes the first n subscriptions *return* after that frame,
      // which is how the daemon dropping the feed reaches the browser.
      streamTerminalOutput: async function* (req) {
        const openIndex = streamedTerminals.length;
        streamedTerminals.push({ sessionId: req.sessionId, terminalId: req.terminalId });
        const terminalId = req.terminalId || "main";
        yield create(SessionTerminalOutputSchema, {
          data: new TextEncoder().encode(`term:${terminalId}\r\n`),
          endOffset: scenario.terminalReplayEndOffset ?? 0n,
          atOldest: (scenario.terminalHistory ?? []).length === 0,
          sessionId: req.sessionId,
          terminalId,
        });
        if (openIndex < (scenario.droppedTerminalStreams ?? 0)) return;
        await new Promise<never>(() => undefined);
      },
      // Lazy scroll-up history — yield one chunk per call from `scenario.terminalHistory` (popped
      // in order), so a test can assert the loader chains anchors across successive calls.
      getTerminalHistory: async function* (req) {
        getTerminalHistoryCalls.push({
          sessionId: req.sessionId,
          terminalId: req.terminalId,
          beforeOffset: req.beforeOffset,
        });
        const chunk = (scenario.terminalHistory ?? []).shift();
        if (chunk) {
          yield create(TerminalHistoryChunkSchema, chunk);
        }
      },
    });

  return Object.assign(backend, {
    deletedSessionIds,
    signalCalls,
    executedToolSessionIds,
    claimedControlSessionIds,
    connectedSessionIds,
    startTerminalSessionIds,
    startedTerminalIds,
    stoppedTerminals,
    sentTerminalInput,
    streamedTerminals,
    getTerminalHistoryCalls,
    ...hostControls,
    ...worktreeControls,
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
