import { describe, expect, it } from "bun:test";
import { aSessionEntry } from "../../../test-utils";
import type { SessionMetadata } from "../../../lib/sessionParticipantMetadata";
import { stackChildSessions } from "./stackChildSessions";

/**
 * Tests for `stackChildSessions` — the adapter that turns the drawer's session list plus the
 * participant metadata it already parses into the one shape the PR-Stack view joins on.
 *
 * The view needs four facts about a child: which session it is, which orchestrator spawned it, which
 * planned node it materializes, and which branch it created. Three of them ride on `SessionEntry`
 * already; `stackNodeId` exists only in the participant's `session` metadata block, because it is
 * needed exactly where a participant is live — a same-host child with no participant needs no such
 * join, since the node's own link was written correctly on its own host (D38).
 *
 * A session that names no orchestrator is not a stack child and is dropped: a `stackNodeId` alone
 * would let one stack's row claim another stack's session, since node ids are unique within a plan
 * and nowhere else.
 *
 * PRD: docs/ft/coder/pr-stack-live-status.md § Cross-host planned PRs (D37, D38).
 */

const ORCHESTRATOR = "pr-stack-session-1";
const CHILD = "dddddddd-0000-4000-8000-000000000004";
const BRANCH = "feature/attach-docs/attach-store";

function aSessionMetadata(overrides: Partial<SessionMetadata> = {}): SessionMetadata {
  return {
    workflowGoal: "",
    workflowState: "",
    agent: "claude",
    model: "sonnet-4",
    activityStatus: "",
    recipe: "tdd",
    repoPath: "/home/dev/pr-stack-project",
    elapsedDisplay: "",
    pendingElicitation: false,
    sessionId: CHILD,
    orchestratorSessionId: ORCHESTRATOR,
    stackNodeId: "n1",
    branch: BRANCH,
    ...overrides,
  };
}

describe("stackChildSessions", () => {
  it("reads a child's node, orchestrator, branch and liveness from its participant metadata", () => {
    // Given — a session known only as a live participant on another host
    const sessions = [
      aSessionEntry({
        sessionId: CHILD,
        isActive: true,
        branch: BRANCH,
        orchestratorSessionId: ORCHESTRATOR,
      }),
    ];
    const metadata = new Map([[CHILD, aSessionMetadata()]]);

    // When
    const children = stackChildSessions(sessions, metadata);

    // Then
    expect(children).toEqual([
      {
        sessionId: CHILD,
        orchestratorSessionId: ORCHESTRATOR,
        stackNodeId: "n1",
        branch: BRANCH,
        isActive: true,
      },
    ]);
  });

  it("takes the orchestrator and branch from the session row when its metadata names none", () => {
    // Given — a same-host child, fully enriched by `ListSessions`, whose participant published no
    // stack association (an older coder, or a session type that publishes no block at all)
    const sessions = [
      aSessionEntry({
        sessionId: CHILD,
        isActive: true,
        branch: BRANCH,
        orchestratorSessionId: ORCHESTRATOR,
      }),
    ];

    // When
    const children = stackChildSessions(sessions, new Map());

    // Then — the node id is simply unknown; the branch still links the child to its row
    expect(children).toEqual([
      {
        sessionId: CHILD,
        orchestratorSessionId: ORCHESTRATOR,
        stackNodeId: "",
        branch: BRANCH,
        isActive: true,
      },
    ]);
  });

  it("prefers the session row's branch over the participant's when both are present", () => {
    // Given — `ListSessions` reads `changeset.yaml` on the session's own host, which is where the
    // branch is written; the metadata block is a copy that a reconnect can republish stale
    const sessions = [
      aSessionEntry({
        sessionId: CHILD,
        branch: BRANCH,
        orchestratorSessionId: ORCHESTRATOR,
      }),
    ];
    const metadata = new Map([[CHILD, aSessionMetadata({ branch: "feature/stale/name" })]]);

    // When
    const children = stackChildSessions(sessions, metadata);

    // Then
    expect(children[0]?.branch).toBe(BRANCH);
  });

  it("drops a session that names no orchestrator", () => {
    // Given — an ordinary session that is nobody's stack child, live and carrying a branch
    const sessions = [aSessionEntry({ sessionId: "sess-plain", isActive: true, branch: "main" })];

    // When
    const children = stackChildSessions(sessions, new Map());

    // Then — a node id alone would let one stack's row claim another stack's session
    expect(children).toEqual([]);
  });

  it("keeps a child whose orchestrator is known only from its participant metadata", () => {
    // Given — the synthesized cross-host row before the drawer hydrates it, or a row that never was
    const sessions = [aSessionEntry({ sessionId: CHILD, isActive: true, branch: "", orchestratorSessionId: "" })];
    const metadata = new Map([[CHILD, aSessionMetadata()]]);

    // When
    const children = stackChildSessions(sessions, metadata);

    // Then
    expect(children).toEqual([
      {
        sessionId: CHILD,
        orchestratorSessionId: ORCHESTRATOR,
        stackNodeId: "n1",
        branch: BRANCH,
        isActive: true,
      },
    ]);
  });

  it("reports an inactive stack child as not live", () => {
    // Given — a finished child, still listed by its own host
    const sessions = [
      aSessionEntry({
        sessionId: CHILD,
        isActive: false,
        branch: BRANCH,
        orchestratorSessionId: ORCHESTRATOR,
      }),
    ];

    // When
    const children = stackChildSessions(sessions, new Map());

    // Then — liveness is a separate fact from the association; the row still binds to it
    expect(children[0]?.isActive).toBe(false);
  });
});
