/**
 * Unit tests for `parseSessionParticipantMetadata` — parses the `session` block from a LiveKit
 * participant's metadata JSON, tolerating missing keys and older empty metadata.
 *
 * Changeset: `2026-07-12-fast-session-change`
 * Feature: `docs/ft/web/session-drawer.md#fast-session-change` (req 4)
 *
 * ⚠️ RED PHASE — fails until `./sessionParticipantMetadata` exists with the API below.
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
});
