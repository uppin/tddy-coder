import { describe, expect, it } from "bun:test";
import { startSessionOverridesFor } from "./branchConflict";

/**
 * The prompt shown when a session creation names a branch another session already owns offers three
 * ways out, and each one has to be expressed in the daemon's existing `StartSession` vocabulary —
 * no new intent is invented. `startSessionOverridesFor` is that translation: it decides whether a
 * choice re-submits a creation at all, and with which branch fields.
 *
 * PRD: docs/ft/daemon/session-branch-conflict.md § Operator prompt
 */

/** The refusal being resolved: `branch` is the branch the owning session holds. */
function aBranchConflict(overrides: Partial<{ branch: string }> = {}) {
  return { branch: "feat/auth", ...overrides };
}

describe("startSessionOverridesFor", () => {
  it("submits no creation when the operator switches to the owning session", () => {
    // Given — the owning session already exists; switching only selects and attaches it
    const conflict = aBranchConflict();

    // When
    const overrides = startSessionOverridesFor({ choice: "switch-to-owner" }, conflict);

    // Then
    expect(overrides).toBeNull();
  });

  it("joins the owned branch when the operator adds another agent", () => {
    // Given
    const conflict = aBranchConflict({ branch: "feat/auth" });

    // When
    const overrides = startSessionOverridesFor({ choice: "add-agent" }, conflict);

    // Then — `work_on_selected_branch` reuses the owner's worktree, and the requested new branch
    // name must be gone or the daemon would still be asked to create it.
    expect(overrides).toEqual({
      branchWorktreeIntent: "work_on_selected_branch",
      selectedBranchToWorkOn: "feat/auth",
      newBranchName: "",
    });
  });

  it("creates a new branch under the name the operator typed when renaming", () => {
    // Given
    const conflict = aBranchConflict({ branch: "feat/auth" });

    // When
    const overrides = startSessionOverridesFor(
      { choice: "rename", branchName: "feat/auth-rewrite" },
      conflict,
    );

    // Then — the same intent as the refused request, under a different name
    expect(overrides).toEqual({
      branchWorktreeIntent: "new_branch_from_base",
      newBranchName: "feat/auth-rewrite",
    });
  });
});
