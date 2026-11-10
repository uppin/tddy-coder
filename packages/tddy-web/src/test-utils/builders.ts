/**
 * Test builders for the bun:test suite.
 *
 * Every factory produces a valid object from zero arguments; pass overrides to highlight
 * only the fields relevant to each test scenario.
 *
 * For bun:test only — never import from Cypress tests.
 *
 * For ConnectRPC stubbing prefer `anInMemoryRpcBackend` (re-exported from
 * `tddy-connectrpc-testkit`).
 */

// Re-export the in-memory ConnectRPC backend as a primary test-utils export.
export { anInMemoryRpcBackend } from "tddy-connectrpc-testkit";

import { create } from "@bufbuild/protobuf";

import { type AgentInfo } from "tddy-client-web/gen/catalog_pb";
import { type ProjectEntry } from "tddy-client-web/gen/project_pb";
import { ProjectEntrySchema } from "tddy-client-web/gen/project_pb";
import { SessionEntrySchema, type SessionEntry } from "tddy-client-web/gen/session_pb";
import { AgentInfoSchema } from "tddy-client-web/gen/catalog_pb";
import { EligibleDaemonEntrySchema, type EligibleDaemonEntry } from "tddy-client-web/gen/host_pb";
import type { ViewRect } from "tddy-terminal-web";
import type { TransitionCounters } from "tddy-terminal-web";
import type { RoomParticipant } from "tddy-livekit-web";

// ---------------------------------------------------------------------------
// Proto: SessionEntry
// ---------------------------------------------------------------------------

/** Default active session; override only the fields that matter for your test. */
export function aSessionEntry(overrides: Partial<SessionEntry> = {}): SessionEntry {
  return create(SessionEntrySchema, {
    sessionId: "session-default-1111-2222-3333-4444444444",
    createdAt: "2026-03-21T10:00:00Z",
    status: "active",
    repoPath: "/home/dev/project",
    pid: 12345,
    isActive: true,
    projectId: "proj-1",
    daemonInstanceId: "",
    workflowGoal: "",
    workflowState: "",
    elapsedDisplay: "",
    agent: "",
    model: "",
    pendingElicitation: false,
    ...overrides,
  });
}

/** Convenience: active session with explicit isActive:true, status:"active", pid:12345. */
export function anActiveSession(overrides: Partial<SessionEntry> = {}): SessionEntry {
  return aSessionEntry({ isActive: true, status: "active", pid: 12345, ...overrides });
}

/** Convenience: inactive/exited session with isActive:false, status:"exited", pid:0. */
export function anInactiveSession(overrides: Partial<SessionEntry> = {}): SessionEntry {
  return aSessionEntry({ isActive: false, status: "exited", pid: 0, ...overrides });
}

// ---------------------------------------------------------------------------
// Proto: ProjectEntry
// ---------------------------------------------------------------------------

export function aProjectEntry(overrides: Partial<ProjectEntry> = {}): ProjectEntry {
  return create(ProjectEntrySchema, {
    projectId: "proj-1",
    name: "Test Project",
    gitUrl: "https://github.com/test/project.git",
    mainRepoPath: "/home/dev/project",
    daemonInstanceId: "",
    ...overrides,
  });
}

// ---------------------------------------------------------------------------
// Proto: AgentInfo
// ---------------------------------------------------------------------------

export function anAgentInfo(overrides: Partial<AgentInfo> = {}): AgentInfo {
  return create(AgentInfoSchema, {
    id: "claude",
    label: "Claude (opus)",
    ...overrides,
  });
}

// ---------------------------------------------------------------------------
// Proto: EligibleDaemonEntry
// ---------------------------------------------------------------------------

export function anEligibleDaemon(overrides: Partial<EligibleDaemonEntry> = {}): EligibleDaemonEntry {
  return create(EligibleDaemonEntrySchema, {
    instanceId: "local",
    label: "local (this daemon)",
    isLocal: true,
    ...overrides,
  });
}

// ---------------------------------------------------------------------------
// ViewRect (terminalStatusBarLayout / terminalPresentation tests)
// ---------------------------------------------------------------------------

export function aViewRect(overrides: Partial<ViewRect> = {}): ViewRect {
  return {
    left: 0,
    top: 0,
    right: 100,
    bottom: 50,
    ...overrides,
  };
}

// ---------------------------------------------------------------------------
// TransitionCounters (terminalPresentation tests)
// ---------------------------------------------------------------------------

export function aConnectCounter(overrides: Partial<TransitionCounters> = {}): TransitionCounters {
  return {
    connectSessionCalls: 0,
    resumeSessionCalls: 0,
    disconnectCalls: 0,
    ...overrides,
  };
}

// ---------------------------------------------------------------------------
// RoomParticipant (participantCameraVideo / useRoomParticipants tests)
// ---------------------------------------------------------------------------

export function aFakeParticipant(overrides: Partial<RoomParticipant> = {}): RoomParticipant {
  return {
    identity: "web-testuser",
    role: "browser",
    joinedAt: 1_700_000_000_000,
    metadata: "",
    codexOAuth: null,
    ...overrides,
  };
}

/** Returns a JSON string for a daemon advertisement in the LiveKit common room. */
export function aDaemonAdvertisementMeta(fields: {
  instanceId?: string;
  label?: string;
} = {}): string {
  return JSON.stringify({
    instance_id: fields.instanceId ?? "my-host",
    label: fields.label ?? "my-host (this daemon)",
  });
}

/** Returns a JSON string for a Codex OAuth metadata payload. */
export function aCodexOAuthMeta(fields: {
  pending?: boolean;
  authorizeUrl?: string;
  callbackPort?: number;
  state?: string;
} = {}): string {
  return JSON.stringify({
    codex_oauth: {
      pending: fields.pending ?? true,
      authorize_url: fields.authorizeUrl ?? "https://auth.example.com/oauth/authorize",
      callback_port: fields.callbackPort ?? 8765,
      state: fields.state ?? "test-state",
    },
  });
}
