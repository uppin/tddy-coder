import { describe, expect, it } from "bun:test";
import { aBranchResolution, aStackChildSession, aStackNode } from "../../../test-utils";
import { isNodeOrphaned } from "./isNodeOrphaned";

/**
 * Tests for `isNodeOrphaned` — whether a planned PR's recorded child session is gone.
 *
 * `DeleteSession` removes a session directory without touching the orchestrator's `Changeset.stack`,
 * so a node keeps a dangling `session_id` forever. The authority on whether that session still
 * exists is `QueryBranch`, which scans sessions by their changeset branch: a resolution reporting no
 * session for the node's branch means the recorded child is gone and the node is workable again.
 *
 * A resolution that has not arrived is "unknown", never "orphaned" — `useQueryBranch` swallows
 * failed polls, so treating an absent resolution as an orphan would offer a duplicate spawn for a
 * node whose child is alive.
 *
 * That authority is **one host's**. `QueryBranch`'s session leg is a `read_dir` over the queried
 * daemon's own sessions directory, so `exists = false` means "not here", which is the normal state
 * for a child running one host over. A child session resolved for the node — from presence, which
 * does cross the host boundary — therefore overrides the verdict: it positively proves the session
 * exists, where the resolution can only report not having found it (D40, amending D7).
 *
 * Only a child resolved **by identity** may do so. "Whoever owns this branch right now" is a
 * different session, and the case where it differs — the recorded child deleted, its branch picked
 * up by a fresh session in the same stack — is precisely the orphan this reports. See
 * `nodeChildSessionByIdentity`.
 *
 * PRD: docs/ft/coder/pr-stack-live-status.md § Cross-host planned PRs (D40).
 */

const OWNED_BRANCH = "feature/attach-docs/attach-store";

describe("isNodeOrphaned", () => {
  it("reports orphaned when the node records a session the resolution says does not exist", () => {
    // Given — the node was spawned once; nothing owns its branch now
    const node = aStackNode({ nodeId: "n1", branch: OWNED_BRANCH, sessionId: "child-since-deleted" });
    const resolution = aBranchResolution({
      branch: OWNED_BRANCH,
      session: { exists: false, sessionId: "", isActive: false, status: "" },
    });

    // When
    const orphaned = isNodeOrphaned(node, resolution);

    // Then
    expect(orphaned).toBe(true);
  });

  it("reports not orphaned when the resolution finds a live session on the branch", () => {
    // Given
    const node = aStackNode({ nodeId: "n1", branch: OWNED_BRANCH, sessionId: "child-n1" });
    const resolution = aBranchResolution({
      branch: OWNED_BRANCH,
      session: { exists: true, sessionId: "child-n1", isActive: true, status: "active" },
    });

    // When
    const orphaned = isNodeOrphaned(node, resolution);

    // Then
    expect(orphaned).toBe(false);
  });

  it("reports not orphaned when the resolution finds an idle session on the branch", () => {
    // Given — a session that exists but is not running still owns the node; it can be resumed
    const node = aStackNode({ nodeId: "n1", branch: OWNED_BRANCH, sessionId: "child-n1" });
    const resolution = aBranchResolution({
      branch: OWNED_BRANCH,
      session: { exists: true, sessionId: "child-n1", isActive: false, status: "idle" },
    });

    // When
    const orphaned = isNodeOrphaned(node, resolution);

    // Then
    expect(orphaned).toBe(false);
  });

  it("reports not orphaned while the resolution has not arrived", () => {
    // Given — the node records a session but nothing is known about it yet
    const node = aStackNode({ nodeId: "n1", branch: OWNED_BRANCH, sessionId: "child-n1" });

    // When
    const orphaned = isNodeOrphaned(node, undefined);

    // Then — an unanswered poll must not be read as a deleted session
    expect(orphaned).toBe(false);
  });

  it("reports not orphaned for a node that was never spawned", () => {
    // Given — no session was ever recorded, so there is nothing to be orphaned from
    const node = aStackNode({ nodeId: "n1", branchSuggestion: OWNED_BRANCH });
    const resolution = aBranchResolution({
      branch: OWNED_BRANCH,
      session: { exists: false, sessionId: "", isActive: false, status: "" },
    });

    // When
    const orphaned = isNodeOrphaned(node, resolution);

    // Then
    expect(orphaned).toBe(false);
  });

  it("reports not orphaned when a child session is resolved for the node on another host", () => {
    // Given — host A cannot see host B's sessions directory, so its verdict is "not here"
    const node = aStackNode({ nodeId: "n1", branch: OWNED_BRANCH, sessionId: "child-on-host-b" });
    const resolution = aBranchResolution({
      branch: OWNED_BRANCH,
      session: { exists: false, sessionId: "", isActive: false, status: "" },
    });
    const child = aStackChildSession({ sessionId: "child-on-host-b", branch: OWNED_BRANCH });

    // When
    const orphaned = isNodeOrphaned(node, resolution, child);

    // Then — offering a second spawn for a session mid-turn is what this prevents
    expect(orphaned).toBe(false);
  });

  it("reports not orphaned when the child that claims the node has already finished", () => {
    // Given — the session exists and is simply idle; only its host can say it is gone
    const node = aStackNode({ nodeId: "n1", branch: OWNED_BRANCH, sessionId: "child-on-host-b" });
    const resolution = aBranchResolution({
      branch: OWNED_BRANCH,
      session: { exists: false, sessionId: "", isActive: false, status: "" },
    });
    const child = aStackChildSession({
      sessionId: "child-on-host-b",
      branch: OWNED_BRANCH,
      isActive: false,
    });

    // When
    const orphaned = isNodeOrphaned(node, resolution, child);

    // Then
    expect(orphaned).toBe(false);
  });

  it("reports orphaned when no child is resolved for the node and the resolution says none exists", () => {
    // Given — the same resolution with nobody claiming the node: the child really is gone
    const node = aStackNode({ nodeId: "n1", branch: OWNED_BRANCH, sessionId: "child-since-deleted" });
    const resolution = aBranchResolution({
      branch: OWNED_BRANCH,
      session: { exists: false, sessionId: "", isActive: false, status: "" },
    });

    // When
    const orphaned = isNodeOrphaned(node, resolution, undefined);

    // Then — presence narrows the rule, it does not remove it
    expect(orphaned).toBe(true);
  });

  it("reports not orphaned for a node that records a session but owns no branch", () => {
    // Given — with no branch there is no join key, so `QueryBranch` never resolves this node
    const node = aStackNode({ nodeId: "n1", sessionId: "child-n1" });

    // When
    const orphaned = isNodeOrphaned(node, undefined);

    // Then
    expect(orphaned).toBe(false);
  });
});
