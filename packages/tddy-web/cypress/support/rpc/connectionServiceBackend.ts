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
  EligibleDaemonEntrySchema,
  ProjectEntrySchema,
  ResumeSessionResponseSchema,
  SessionEntrySchema,
  StartSessionResponseSchema,
  ToolInfoSchema,
  ClaimTerminalControlResponseSchema,
  ExecuteToolResponseSchema,
  GetHostCpuStatsResponseSchema,
  GetHostDiskStatsResponseSchema,
  ListExecToolsResponseSchema,
  ListSessionToolCallsResponseSchema,
  ListTerminalSessionsResponseSchema,
  SessionTerminalOutputSchema,
  StartTerminalSessionResponseSchema,
  StopTerminalSessionResponseSchema,
  TerminalSessionInfoSchema,
  ToolDefSchema,
  type AgentInfo,
  type ConnectSessionResponse,
  type EligibleDaemonEntry,
  type ProjectEntry,
  type ResumeSessionResponse,
  type SessionEntry,
  type StartSessionResponse,
} from "../../../src/gen/connection_pb";
import { aGitHubUser, DEFAULT_AGENTS } from "./responses";

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

export interface DaemonEntry {
  instanceId: string;
  label: string;
  isLocal: boolean;
}

/** Canonical daemon pair for multi-host tests (same values as the retired `connectionRpcs.ts`). */
export const DAEMON_LOCAL: DaemonEntry = {
  instanceId: "workstation-1",
  label: "workstation-1 (this daemon)",
  isLocal: true,
};

export const DAEMON_PEER: DaemonEntry = {
  instanceId: "server-2",
  label: "server-2",
  isLocal: false,
};

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

function anEligibleDaemonEntry(overrides: Partial<EligibleDaemonEntry>): EligibleDaemonEntry {
  return create(EligibleDaemonEntrySchema, {
    instanceId: "local",
    label: "local (this daemon)",
    isLocal: true,
    ...overrides,
  });
}

// ---------------------------------------------------------------------------
// Scenario options
// ---------------------------------------------------------------------------

export interface ConnectionServiceScenario {
  /** Static ListSessions response. Ignored when `listSessionsFactory` is given. */
  sessions?: Partial<SessionEntry>[];
  /** Dynamic ListSessions response, re-evaluated on every call (poll-driven tests). */
  listSessionsFactory?: () => Partial<SessionEntry>[];
  /** ListEligibleDaemons response. Defaults to a single local daemon. */
  daemons?: DaemonEntry[];
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
  /** `GetHostCpuStats` response — per-logical-core utilization percentages (0..100). Default empty. */
  hostCpuPerCore?: number[];
  /** `GetHostDiskStats` response — free/total bytes for the daemon's default project directory. */
  hostDisk?: { availableBytes: bigint; totalBytes: bigint; projectDir: string };
}

export interface ConnectionServiceBackend extends InMemoryRpcBackend {
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
  /** Number of `GetHostCpuStats` calls — lets tests assert the 5 s poll cadence. */
  readonly hostCpuStatsCallCount: () => number;
  /** Number of `GetHostDiskStats` calls — lets tests assert the 60 s poll cadence. */
  readonly hostDiskStatsCallCount: () => number;
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
  let hostCpuCalls = 0;
  let hostDiskCalls = 0;

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

  const backend = anInMemoryRpcBackend()
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
        models: [
          { id: "claude-opus-4-8", label: "Claude Opus 4.8" },
          { id: "claude-sonnet-4-6", label: "Claude Sonnet 4.6" },
          { id: "claude-haiku-4-5-20251001", label: "Claude Haiku 4.5" },
        ],
        defaultModel: "claude-opus-4-8",
      }),
      listEligibleDaemons: async () => ({
        daemons: daemons.map((d) => anEligibleDaemonEntry(d)),
      }),
      listSessions: async () => ({
        sessions: (scenario.listSessionsFactory ? scenario.listSessionsFactory() : (scenario.sessions ?? [])).map(
          (s) => aSessionEntry(s),
        ),
      }),
      listProjects: async () => ({
        projects: projectsOverride.map((p) => aProjectEntry(p)),
      }),
      listProjectBranches: async () => ({ branches: scenario.projectBranches ?? [] }),
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
      // --- Host stats footer ---
      getHostCpuStats: async () => {
        hostCpuCalls += 1;
        return create(GetHostCpuStatsResponseSchema, {
          perCorePercent: scenario.hostCpuPerCore ?? [],
        });
      },
      getHostDiskStats: async () => {
        hostDiskCalls += 1;
        return create(GetHostDiskStatsResponseSchema, {
          availableBytes: scenario.hostDisk?.availableBytes ?? 0n,
          totalBytes: scenario.hostDisk?.totalBytes ?? 0n,
          projectDir: scenario.hostDisk?.projectDir ?? "",
        });
      },
      // Server-streaming output — record the opened stream, emit one identifying frame, then stay
      // open (a terminal stream that *completes* would signal disconnect and evict the runtime).
      streamTerminalOutput: async function* (req) {
        streamedTerminals.push({ sessionId: req.sessionId, terminalId: req.terminalId });
        yield create(SessionTerminalOutputSchema, {
          data: new TextEncoder().encode(`term:${req.terminalId || "main"}\r\n`),
        });
        await new Promise<never>(() => undefined);
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
    hostCpuStatsCallCount: () => hostCpuCalls,
    hostDiskStatsCallCount: () => hostDiskCalls,
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
