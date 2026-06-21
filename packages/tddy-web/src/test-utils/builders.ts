/**
 * Test builders for the bun:test suite.
 *
 * Every factory produces a valid object from zero arguments; pass overrides to highlight
 * only the fields relevant to each test scenario.
 *
 * For bun:test only — never import from Cypress tests.
 */

import { create } from "@bufbuild/protobuf";
import type { Transport, UnaryResponse } from "@connectrpc/connect";
import { ConnectError, type Code } from "@connectrpc/connect";
import type { DescMethod } from "@bufbuild/protobuf";

import {
  AgentInfoSchema,
  EligibleDaemonEntrySchema,
  ProjectEntrySchema,
  SessionEntrySchema,
  type AgentInfo,
  type EligibleDaemonEntry,
  type ProjectEntry,
  type SessionEntry,
} from "../gen/connection_pb";
import type { ViewRect } from "../lib/terminalStatusBarLayout";
import type { TransitionCounters } from "../components/connection/terminalPresentation";
import type { RoomParticipant } from "../hooks/useRoomParticipants";

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

// ---------------------------------------------------------------------------
// RPC Playground: registry helpers
// ---------------------------------------------------------------------------

/**
 * Returns a registry pre-loaded with the EchoService descriptor fixture.
 * Requires registry.ts and the echoServiceDescriptor fixture to exist (Green phase).
 */
export async function anEchoRegistry() {
  const { buildRegistry } = await import("../rpc-playground/registry");
  const { ECHO_SERVICE_DESCRIPTOR_FIXTURE } = await import(
    "../rpc-playground/test-fixtures/echoServiceDescriptor"
  );
  return buildRegistry(ECHO_SERVICE_DESCRIPTOR_FIXTURE);
}

// ---------------------------------------------------------------------------
// RPC Playground: fake transports
// ---------------------------------------------------------------------------

/**
 * Fake unary Transport that returns a preset JSON response string decoded through the method's
 * output schema — matches the `makeFakeUnaryTransport` pattern in invoke.test.ts.
 */
export function aFakeUnaryTransport(responseJson: string): Transport {
  return {
    async unary(method: DescMethod): Promise<UnaryResponse> {
      const { fromJsonString } = await import("@bufbuild/protobuf");
      const output = fromJsonString(method.output, responseJson);
      return {
        stream: false,
        service: method.parent,
        method,
        header: new Headers(),
        trailer: new Headers(),
        message: output,
      } as unknown as UnaryResponse;
    },
    stream() {
      throw new Error("stream not expected in this transport");
    },
  } as unknown as Transport;
}

/** Fake Transport that rejects every call with a ConnectError of the given code. */
export function aFakeErrorTransport(code: Code): Transport {
  return {
    async unary() {
      throw new ConnectError("test error", code);
    },
    stream() {
      throw new ConnectError("test error", code);
    },
  } as unknown as Transport;
}

/**
 * Fake server-streaming Transport that yields each string in `chunks` decoded through the
 * method's output schema — matches the inline streamingTransport in invoke.test.ts.
 */
export function aFakeStreamingTransport(chunks: string[]): Transport {
  return {
    unary() {
      throw new Error("unary not expected in this transport");
    },
    async stream(m: DescMethod) {
      const { fromJsonString } = await import("@bufbuild/protobuf");
      return {
        stream: true,
        method: m,
        header: new Headers(),
        trailer: new Headers(),
        message: (async function* () {
          for (const chunk of chunks) {
            yield fromJsonString(m.output, chunk);
          }
        })(),
      };
    },
  } as unknown as Transport;
}
