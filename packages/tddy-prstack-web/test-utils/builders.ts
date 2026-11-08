/**
 * Test builders for the tddy-prstack-web bun:test suites. Every factory produces a valid object from
 * zero arguments; pass overrides to highlight only the fields relevant to a scenario.
 * For bun:test only — never import from Cypress tests.
 */

import { create, type MessageInitShape } from "@bufbuild/protobuf";
import { BranchResolutionSchema, type BranchResolution } from "tddy-client-web/gen/pr_stack_pb";
import { SessionEntrySchema, type SessionEntry } from "tddy-client-web/gen/session_pb";
import type { StackChildSession } from "../src/stackChildSessions";
import type { StackNode } from "../src/stackPlan";

/** Default active session; override only the fields that matter for your test. */
export function aSessionEntry(overrides: MessageInitShape<typeof SessionEntrySchema> = {}): SessionEntry {
  return create(SessionEntrySchema, {
    sessionId: "session-default-1111-2222-3333-4444444444",
    createdAt: "2026-03-21T10:00:00Z",
    status: "active",
    repoPath: "/home/dev/project",
    pid: 12345,
    isActive: true,
    projectId: "proj-1",
    ...overrides,
  });
}

// ---------------------------------------------------------------------------
// Proto: BranchResolution (QueryBranch)
// ---------------------------------------------------------------------------

/**
 * A `QueryBranch` resolution with every leg present and empty — no session, no worktree, no PR, and
 * no branch on `origin`. Override only the leg a scenario cares about. Every leg is set explicitly
 * rather than left `undefined`, because "the leg is absent" and "the leg says nothing exists" are
 * different conditions in the PR-Stack view — the `remote` leg most of all, since a *missing*
 * resolution leaves a node startable while `remote.exists === false` blocks it.
 */
export function aBranchResolution(
  overrides: MessageInitShape<typeof BranchResolutionSchema> = {},
): BranchResolution {
  return create(BranchResolutionSchema, {
    branch: "feature/x/n1",
    session: { exists: false, sessionId: "", isActive: false, status: "" },
    worktree: { exists: false, path: "" },
    pr: { exists: false, number: 0n, url: "", state: "" },
    remote: { exists: false, sha: "" },
    // A built message among the overrides carries `$typeName`, which narrows the spread to a type
    // `create` will not take; every field it carries is still a valid init.
    ...(overrides as Omit<typeof overrides, "$typeName">),
  });
}

// ---------------------------------------------------------------------------
// StackNode (PR-Stack planned PR)
// ---------------------------------------------------------------------------

/**
 * A planned PR as `parseStackPlan` produces it — every field present and carrying the value an
 * unspawned, branchless, unordered node has. Override only the fields a scenario is about.
 *
 * `displayOrder` defaults to `null` (the plan orders nothing) rather than being left off: absent and
 * `null` mean the same thing to `orderStackNodes` but are different *values*, and only `parseStackPlan`
 * guarantees the field is written. A test about the absent-field case builds it explicitly.
 *
 * The title defaults to the node id so a stack built from ids alone still renders distinguishable
 * rows; a test that reads titles states them.
 */
export function aStackNode(overrides: Partial<StackNode> = {}): StackNode {
  const nodeId = overrides.nodeId ?? "n1";
  return {
    nodeId,
    title: nodeId,
    description: "",
    branchSuggestion: null,
    branch: null,
    sessionId: null,
    parents: [],
    prStatus: null,
    childState: null,
    childRecipe: "tdd",
    internalStatus: null,
    displayOrder: null,
    ...overrides,
  };
}

// ---------------------------------------------------------------------------
// StackChildSession (a session working a planned PR)
// ---------------------------------------------------------------------------

/**
 * A live child session of a pr-stack orchestrator, in the shape the view joins on
 * (`stackChildSessions`). Every field present: the association is a set of identities, and "absent"
 * and "empty" are the same value here but never the same *statement* — an empty `stackNodeId` means
 * "claims no node" and must never read as a wildcard.
 *
 * Defaults describe the ordinary case: a live child that names both its orchestrator and its node,
 * and owns the branch it created. A scenario states only the identity it is about — which is what
 * separates the three join legs from each other, since each is defined by which of these fields the
 * child does *not* carry.
 */
export function aStackChildSession(
  overrides: Partial<StackChildSession> = {},
): StackChildSession {
  return {
    sessionId: "child-session-1",
    orchestratorSessionId: "pr-stack-session-1",
    stackNodeId: "n1",
    branch: "feature/stack/n1",
    isActive: true,
    ...overrides,
  };
}
