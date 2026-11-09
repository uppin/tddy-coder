/**
 * Unit tests for `parseSessionParticipantMetadata` — parses the `session` block from a LiveKit
 * participant's metadata JSON, tolerating missing keys and older empty metadata.
 *
 * Changeset: `2026-07-12-fast-session-change`, `2026-08-30-cross-host-planned-pr-visibility`
 * Feature: `docs/ft/web/session-drawer.md#fast-session-change` (req 4),
 *          `docs/ft/coder/pr-stack-live-status.md` § Cross-host planned PRs (D37)
 *
 * The block also carries the session's **stack association** — its own id, the orchestrator that
 * spawned it, the planned node it materializes and the branch it created. Presence is the only
 * cross-host signal the web has, so this is the only place the PR-Stack view can learn which planned
 * PR a session on another host is working.
 */

import { describe, it, expect } from "bun:test";
import { parseSessionParticipantMetadata } from "./sessionParticipantMetadata";

describe("parseSessionParticipantMetadata", () => {
  it("parses a full session block into typed fields", () => {
    // Given
    const metadata = JSON.stringify({
      session: {
        workflow_goal: "acceptance-tests",
        workflow_state: "Red",
        elapsed_display: "3m",
        agent: "claude",
        model: "sonnet-4",
        activity_status: "",
        recipe: "tdd",
        repo_path: "/home/dev/feature",
        pending_elicitation: false,
        session_id: "dddddddd-0000-4000-8000-000000000004",
        orchestrator_session_id: "pr-stack-session-1",
        stack_node_id: "n2",
        branch: "feature/attach-docs/attach-store",
      },
    });

    // When
    const parsed = parseSessionParticipantMetadata(metadata);

    // Then
    expect(parsed).toEqual({
      workflowGoal: "acceptance-tests",
      workflowState: "Red",
      agent: "claude",
      model: "sonnet-4",
      activityStatus: "",
      recipe: "tdd",
      repoPath: "/home/dev/feature",
      elapsedDisplay: "3m",
      pendingElicitation: false,
      sessionId: "dddddddd-0000-4000-8000-000000000004",
      orchestratorSessionId: "pr-stack-session-1",
      stackNodeId: "n2",
      branch: "feature/attach-docs/attach-store",
    });
  });

  it("returns null when the metadata has no session block", () => {
    // Given — a participant advertising only owned_project_count
    const metadata = JSON.stringify({ owned_project_count: 3 });

    // When
    const parsed = parseSessionParticipantMetadata(metadata);

    // Then
    expect(parsed).toBeNull();
  });

  it("returns null for empty or whitespace metadata (older participants)", () => {
    expect(parseSessionParticipantMetadata("")).toBeNull();
    expect(parseSessionParticipantMetadata("   ")).toBeNull();
  });

  it("tolerates a session block with missing optional keys by defaulting them", () => {
    // Given — only goal and state are present
    const metadata = JSON.stringify({
      session: { workflow_goal: "plan", workflow_state: "Plan" },
    });

    // When
    const parsed = parseSessionParticipantMetadata(metadata);

    // Then — present keys parse; absent keys default
    expect(parsed?.workflowGoal).toBe("plan");
    expect(parsed?.workflowState).toBe("Plan");
    expect(parsed?.agent).toBe("");
    expect(parsed?.model).toBe("");
    expect(parsed?.pendingElicitation).toBe(false);
  });

  it("defaults the stack association to empty for a participant that publishes none", () => {
    // Given — a session that is not a stack child, or a coder that predates the association
    const metadata = JSON.stringify({
      session: { workflow_goal: "plan", workflow_state: "Plan" },
    });

    // When
    const parsed = parseSessionParticipantMetadata(metadata);

    // Then — empty is "no association", never a partial one that could match a node by accident
    expect(parsed?.sessionId).toBe("");
    expect(parsed?.orchestratorSessionId).toBe("");
    expect(parsed?.stackNodeId).toBe("");
    expect(parsed?.branch).toBe("");
  });

  it("parses the stack association of a child that has published nothing else yet", () => {
    // Given — the daemon publishes a claude-cli session's block at spawn, before any workflow event
    const metadata = JSON.stringify({
      session: {
        recipe: "tdd",
        repo_path: "/home/dev/feature",
        session_id: "dddddddd-0000-4000-8000-000000000004",
        orchestrator_session_id: "pr-stack-session-1",
        stack_node_id: "n2",
        branch: "feature/attach-docs/attach-store",
      },
    });

    // When
    const parsed = parseSessionParticipantMetadata(metadata);

    // Then
    expect(parsed?.sessionId).toBe("dddddddd-0000-4000-8000-000000000004");
    expect(parsed?.stackNodeId).toBe("n2");
    expect(parsed?.orchestratorSessionId).toBe("pr-stack-session-1");
    expect(parsed?.branch).toBe("feature/attach-docs/attach-store");
    expect(parsed?.workflowGoal).toBe("");
  });
});
