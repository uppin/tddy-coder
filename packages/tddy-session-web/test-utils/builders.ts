/**
 * Test builders for the tddy-session-web bun:test suites. Every factory produces a valid object from
 * zero arguments; pass overrides to highlight only the fields relevant to a scenario.
 * For bun:test only — never import from Cypress tests.
 */

import "./matchers";
import { create, type MessageInitShape } from "@bufbuild/protobuf";
import { ProjectEntrySchema, type ProjectEntry } from "tddy-client-web/gen/project_pb";
import { SessionEntrySchema, type SessionEntry } from "tddy-client-web/gen/session_pb";

/**
 * Overrides are message inits, not `Partial<Message>`: a built message's `$typeName` is dropped from
 * the spread below, so plain literals and built messages are both accepted.
 */
type SessionEntryInit = Omit<MessageInitShape<typeof SessionEntrySchema>, "$typeName">;
type ProjectEntryInit = Omit<MessageInitShape<typeof ProjectEntrySchema>, "$typeName">;

/** Default active session; override only the fields that matter for your test. */
export function aSessionEntry(overrides: SessionEntryInit = {}): SessionEntry {
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
export function anActiveSession(overrides: SessionEntryInit = {}): SessionEntry {
  return aSessionEntry({ isActive: true, status: "active", pid: 12345, ...overrides });
}

/** Convenience: inactive/exited session with isActive:false, status:"exited", pid:0. */
export function anInactiveSession(overrides: SessionEntryInit = {}): SessionEntry {
  return aSessionEntry({ isActive: false, status: "exited", pid: 0, ...overrides });
}


export function aProjectEntry(overrides: ProjectEntryInit = {}): ProjectEntry {
  return create(ProjectEntrySchema, {
    projectId: "proj-1",
    name: "Test Project",
    gitUrl: "https://github.com/test/project.git",
    mainRepoPath: "/home/dev/project",
    daemonInstanceId: "",
    ...overrides,
  });
}

