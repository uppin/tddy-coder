import { describe, expect, it } from "bun:test";
import { anActiveSession, anInactiveSession } from "../test-utils";
import { sessionDrawerLabel } from "./sessionDrawerLabel";

describe("sessionDrawerLabel — derived display label for drawer items", () => {
  it("returns the worktree basename from repoPath when it is non-empty", () => {
    // Given
    const session = anActiveSession({ repoPath: "/home/dev/my-feature-worktree", workflowGoal: "", sessionId: "aaaaaaaa-0000-0000-0000-000000000000" });

    // When
    const label = sessionDrawerLabel(session);

    // Then
    expect(label).toBe("my-feature-worktree");
  });

  it("returns the basename of a deeply nested repoPath", () => {
    // Given
    const session = anActiveSession({ repoPath: "/var/tddy/Code/tddy-coder-worktrees/ui-revamp", workflowGoal: "" });

    // When
    const label = sessionDrawerLabel(session);

    // Then
    expect(label).toBe("ui-revamp");
  });

  it("returns the workflowGoal when repoPath is empty", () => {
    // Given
    const session = anActiveSession({ repoPath: "", workflowGoal: "Add session drawer UI" });

    // When
    const label = sessionDrawerLabel(session);

    // Then
    expect(label).toBe("Add session drawer UI");
  });

  it("returns the workflowGoal when repoPath is whitespace only", () => {
    // Given
    const session = anActiveSession({ repoPath: "   ", workflowGoal: "Fix the build" });

    // When
    const label = sessionDrawerLabel(session);

    // Then
    expect(label).toBe("Fix the build");
  });

  it("returns the first 8 characters of sessionId when both repoPath and workflowGoal are empty", () => {
    // Given
    const session = anActiveSession({ repoPath: "", workflowGoal: "", sessionId: "01934567-abcd-0000-0000-000000000000" });

    // When
    const label = sessionDrawerLabel(session);

    // Then
    expect(label).toBe("01934567");
  });

  it("prefers repoPath basename over workflowGoal when both are present", () => {
    // Given
    const session = anActiveSession({ repoPath: "/home/dev/worktree-alpha", workflowGoal: "Some goal that should not appear" });

    // When
    const label = sessionDrawerLabel(session);

    // Then
    expect(label).toBe("worktree-alpha");
  });

  it("works for inactive sessions the same way as active ones", () => {
    // Given
    const session = anInactiveSession({ repoPath: "/srv/project/offline-branch", workflowGoal: "" });

    // When
    const label = sessionDrawerLabel(session);

    // Then
    expect(label).toBe("offline-branch");
  });

  it("handles a repoPath that is just a top-level slash (no basename)", () => {
    // Given — degenerate path, falls back to workflowGoal
    const session = anActiveSession({ repoPath: "/", workflowGoal: "root session goal" });

    // When
    const label = sessionDrawerLabel(session);

    // Then
    expect(label).toBe("root session goal");
  });

  it("falls back all the way to sessionId slice when repoPath is slash and workflowGoal is empty", () => {
    // Given
    const session = anActiveSession({ repoPath: "/", workflowGoal: "", sessionId: "deadbeef-1234-5678-9abc-def012345678" });

    // When
    const label = sessionDrawerLabel(session);

    // Then
    expect(label).toBe("deadbeef");
  });
});
