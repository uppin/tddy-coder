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
}

export interface ConnectionServiceBackend extends InMemoryRpcBackend {
  /** Every `sessionId` passed to `DeleteSession`, in call order. */
  readonly deletedSessionIds: string[];
  /** Every `{ sessionId, signal }` passed to `SignalSession`, in call order. */
  readonly signalCalls: { sessionId: string; signal: number }[];
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

  return Object.assign(backend, { deletedSessionIds, signalCalls });
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
