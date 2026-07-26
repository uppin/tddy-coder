import { describe, expect, it } from "bun:test";
import { localBranchName } from "./branchNames";

/**
 * `ListProjectBranches` lists remote-tracking refs (`git branch -r` over `refs/remotes/origin`), so
 * every branch it offers is `origin/`-prefixed, while the rest of the domain — a stack node's
 * `branch`, a session's `branch` — names the local branch. `localBranchName` is the one place that
 * reduces the former to the latter so the two can be compared.
 */
describe("localBranchName", () => {
  it("strips the remote prefix from a remote-tracking name", () => {
    // Given / When
    const branch = localBranchName("origin/feature/attach-docs/attach-store");

    // Then
    expect(branch).toBe("feature/attach-docs/attach-store");
  });

  it("leaves a name that is already local unchanged", () => {
    // Given / When
    const branch = localBranchName("feature/attach-docs/attach-store");

    // Then
    expect(branch).toBe("feature/attach-docs/attach-store");
  });

  it("strips only one remote prefix so a local branch called origin/… keeps its name", () => {
    // Given / When — `refs/heads/origin/legacy` is a legal local branch
    const branch = localBranchName("origin/origin/legacy");

    // Then
    expect(branch).toBe("origin/legacy");
  });

  it("trims surrounding whitespace", () => {
    // Given / When
    const branch = localBranchName("  origin/master  ");

    // Then
    expect(branch).toBe("master");
  });

  it("reduces an empty reference to an empty name", () => {
    // Given / When — an absent pre-fill, which must not match any offered branch
    const branch = localBranchName("");

    // Then
    expect(branch).toBe("");
  });
});
