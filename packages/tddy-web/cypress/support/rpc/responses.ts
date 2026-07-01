/**
 * Proto response builders for Cypress intercepts.
 *
 * Each factory produces a ready-to-encode protobuf message with sensible defaults;
 * override only the fields that matter for the scenario under test.
 *
 * The canonical "testuser / Test User / id 42" GitHub user literal lives here once.
 */

import { create, toBinary } from "@bufbuild/protobuf";

import {
  AgentInfoSchema,
  ConnectSessionResponseSchema,
  EligibleDaemonEntrySchema,
  ListAgentsResponseSchema,
  ListEligibleDaemonsResponseSchema,
  ListProjectBranchesResponseSchema,
  ListProjectsResponseSchema,
  ListSessionsResponseSchema,
  ListToolsResponseSchema,
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
import { GetAuthStatusResponseSchema, GitHubUserSchema, type GitHubUser } from "../../../src/gen/auth_pb";
import {
  GenerateTokenResponseSchema,
  RefreshTokenResponseSchema,
  type GenerateTokenResponse,
  type RefreshTokenResponse,
} from "../../../src/gen/token_pb";

import { toArrayBuffer } from "./protoRpc";

// ---------------------------------------------------------------------------
// Auth
// ---------------------------------------------------------------------------

/** The canonical test GitHub user — appears exactly once. */
export function aGitHubUser(overrides: Partial<GitHubUser> = {}): GitHubUser {
  return create(GitHubUserSchema, {
    login: "testuser",
    avatarUrl: "https://example.com/avatar.png",
    name: "Test User",
    id: BigInt(42),
    ...overrides,
  });
}

/** Encoded GetAuthStatusResponse with authenticated:true and the test user. */
export function anAuthStatusAuthenticated(userOverrides: Partial<GitHubUser> = {}): Uint8Array {
  return toBinary(
    GetAuthStatusResponseSchema,
    create(GetAuthStatusResponseSchema, {
      authenticated: true,
      user: aGitHubUser(userOverrides),
    }),
  );
}

/** Encoded GetAuthStatusResponse with authenticated:false. */
export function anAuthStatusUnauthenticated(): Uint8Array {
  return toBinary(
    GetAuthStatusResponseSchema,
    create(GetAuthStatusResponseSchema, { authenticated: false }),
  );
}

// ---------------------------------------------------------------------------
// Token
// ---------------------------------------------------------------------------

/** Default presence token values. */
const DEFAULT_TOKEN = "mock-jwt-presence";
const DEFAULT_TTL = 600n;

export function aGenerateTokenResponse(
  overrides: Partial<GenerateTokenResponse> = {},
): Uint8Array {
  return toBinary(
    GenerateTokenResponseSchema,
    create(GenerateTokenResponseSchema, {
      token: DEFAULT_TOKEN,
      ttlSeconds: DEFAULT_TTL,
      ...overrides,
    }),
  );
}

export function aRefreshTokenResponse(
  overrides: Partial<RefreshTokenResponse> = {},
): Uint8Array {
  return toBinary(
    RefreshTokenResponseSchema,
    create(RefreshTokenResponseSchema, {
      token: DEFAULT_TOKEN,
      ttlSeconds: DEFAULT_TTL,
      ...overrides,
    }),
  );
}

// ---------------------------------------------------------------------------
// Connection sessions / projects
// ---------------------------------------------------------------------------

export function listSessions(sessions: Partial<SessionEntry>[]): Uint8Array {
  return toBinary(
    ListSessionsResponseSchema,
    create(ListSessionsResponseSchema, {
      sessions: sessions.map((s) =>
        create(SessionEntrySchema, {
          sessionId: "session-default-1",
          createdAt: "2026-03-21T10:00:00Z",
          status: "active",
          repoPath: "/home/dev/project",
          pid: 12345,
          isActive: true,
          projectId: "proj-1",
          daemonInstanceId: "",
          pendingElicitation: false,
          ...s,
        }),
      ),
    }),
  );
}

export function listProjects(projects: Partial<ProjectEntry>[]): Uint8Array {
  return toBinary(
    ListProjectsResponseSchema,
    create(ListProjectsResponseSchema, {
      projects: projects.map((p) =>
        create(ProjectEntrySchema, {
          projectId: "proj-1",
          name: "Test Project",
          gitUrl: "https://github.com/test/project.git",
          mainRepoPath: "/home/dev/project",
          daemonInstanceId: "",
          ...p,
        }),
      ),
    }),
  );
}

export function listAgents(agents: Partial<AgentInfo>[]): Uint8Array {
  return toBinary(
    ListAgentsResponseSchema,
    create(ListAgentsResponseSchema, {
      agents: agents.map((a) =>
        create(AgentInfoSchema, {
          id: "claude",
          label: "Claude (opus)",
          ...a,
        }),
      ),
    }),
  );
}

export function listEligibleDaemons(daemons: Partial<EligibleDaemonEntry>[]): Uint8Array {
  return toBinary(
    ListEligibleDaemonsResponseSchema,
    create(ListEligibleDaemonsResponseSchema, {
      daemons: daemons.map((d) =>
        create(EligibleDaemonEntrySchema, {
          instanceId: "local",
          label: "local (this daemon)",
          isLocal: true,
          ...d,
        }),
      ),
    }),
  );
}

export function listTools(
  tools: Array<{ path: string; label: string }> = [
    { path: "/usr/bin/tddy-coder", label: "tddy-coder" },
  ],
): Uint8Array {
  return toBinary(
    ListToolsResponseSchema,
    create(ListToolsResponseSchema, {
      tools: tools.map((t) => create(ToolInfoSchema, t)),
    }),
  );
}

// ---------------------------------------------------------------------------
// Connect / Start / Resume
// ---------------------------------------------------------------------------

export function aConnectSessionResponse(
  overrides: Partial<ConnectSessionResponse> = {},
): Uint8Array {
  return toBinary(
    ConnectSessionResponseSchema,
    create(ConnectSessionResponseSchema, {
      livekitRoom: "session-room-ct",
      livekitUrl: "ws://127.0.0.1:7880",
      livekitServerIdentity: "server",
      ...overrides,
    }),
  );
}

export function aStartSessionResponse(
  overrides: Partial<StartSessionResponse> = {},
): Uint8Array {
  return toBinary(
    StartSessionResponseSchema,
    create(StartSessionResponseSchema, {
      sessionId: "session-started-1",
      livekitRoom: "session-room-ct",
      livekitUrl: "ws://127.0.0.1:7880",
      livekitServerIdentity: "server",
      ...overrides,
    }),
  );
}

export function aResumeSessionResponse(
  overrides: Partial<ResumeSessionResponse> = {},
): Uint8Array {
  return toBinary(
    ResumeSessionResponseSchema,
    create(ResumeSessionResponseSchema, {
      sessionId: "session-resumed-1",
      livekitRoom: "resume-room-ct",
      livekitUrl: "ws://127.0.0.1:7880",
      livekitServerIdentity: "server",
      ...overrides,
    }),
  );
}

// ---------------------------------------------------------------------------
// Wire-encoded boolean responses (SignalSession, DeleteSession)
// ---------------------------------------------------------------------------

/** Protobuf encoding of `{ ok: true }` — field 1 varint 1. */
export function listProjectBranches(branches: string[] = []): Uint8Array {
  return toBinary(
    ListProjectBranchesResponseSchema,
    create(ListProjectBranchesResponseSchema, { branches }),
  );
}

export const OK_RESPONSE_BYTES = new Uint8Array([0x08, 0x01]);

export function okResponseBuffer(): ArrayBuffer {
  return toArrayBuffer(OK_RESPONSE_BYTES);
}

// ---------------------------------------------------------------------------
// Default agent list matching dev.daemon.yaml `allowed_agents`
// ---------------------------------------------------------------------------

export const DEFAULT_AGENTS: Array<{ id: string; label: string }> = [
  { id: "claude", label: "Claude (opus)" },
  { id: "claude-acp", label: "Claude ACP (opus)" },
  { id: "cursor", label: "Cursor (composer-2.5)" },
  { id: "stub", label: "Stub" },
  { id: "codex", label: "Codex" },
  { id: "codex-acp", label: "Codex ACP" },
];

export function listDefaultAgents(): Uint8Array {
  return listAgents(DEFAULT_AGENTS);
}
