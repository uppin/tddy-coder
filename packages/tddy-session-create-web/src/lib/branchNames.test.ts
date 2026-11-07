import { describe, expect, it } from "bun:test";
import { localBranchName, remoteTrackingName } from "./branchNames";

/**
 * `ListProjectBranches` lists remote-tracking refs (`git branch -r` over `refs/remotes/<remote>`), so
 * every branch it offers is `<remote>/`-prefixed, while the rest of the domain — a stack node's
 * `branch`, a session's `branch` — names the local branch. `localBranchName` is the one place that
 * reduces the former to the latter so the two can be compared. The remote is supplied by the caller
 * (from `ListProjectBranchesResponse.defaultRemote`); `origin` is only the default fallback.
 */
describe("localBranchName", () => {
  it("strips the remote prefix from a remote-tracking name", () => {
    // Given / When
    const branch = localBranchName("origin/feature/attach-docs/attach-store");

    // Then
    expect(branch).toBe("feature/attach-docs/attach-store");
  });

  it("strips a non-origin remote prefix when the resolved remote is supplied", () => {
    // Given — a project whose default remote is `upstream`
    // When
    const branch = localBranchName("upstream/feature/attach-docs/attach-store", "upstream");

    // Then
    expect(branch).toBe("feature/attach-docs/attach-store");
  });

  it("leaves a foreign remote prefix intact so a mismatch is detectable", () => {
    // Given — the picker offered an `origin/*` ref but the project's remote is `upstream`
    // When
    const branch = localBranchName("origin/feature/x", "upstream");

    // Then
    expect(branch).toBe("origin/feature/x");
  });

  it("leaves a name that is already local unchanged", () => {
    // Given / When
    const branch = localBranchName("feature/attach-docs/attach-store");

    // Then
    expect(branch).toBe("feature/attach-docs/attach-store");
  });

  it("strips only one remote prefix so a local branch called <remote>/… keeps its name", () => {
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

/**
 * `remoteTrackingName` is the inverse of `localBranchName`: it lifts a local branch name (the form a
 * stack node's `branch` or a session's `branch` carries) into the `<remote>/<branch>` remote-tracking
 * ref the daemon's `selected_integration_base_ref` requires. The remote is supplied by the caller
 * (from `ProjectEntry.defaultRemote` / `ListProjectBranchesResponse.defaultRemote`); `origin` is only
 * the default fallback. It is idempotent so it is safe to apply to a value that may already be
 * remote-tracking (e.g. `ProjectEntry.main_branch_ref`).
 */
describe("remoteTrackingName", () => {
  it("prepends the remote prefix to a local branch name", () => {
    // Given / When — the form a stack node's `branch` carries
    const ref = remoteTrackingName("feature/attach-docs/attach-store");

    // Then
    expect(ref).toBe("origin/feature/attach-docs/attach-store");
  });

  it("prepends a non-origin remote when the resolved remote is supplied", () => {
    // Given — a project whose default remote is `upstream`
    // When
    const ref = remoteTrackingName("feature/attach-docs/attach-store", "upstream");

    // Then
    expect(ref).toBe("upstream/feature/attach-docs/attach-store");
  });

  it("leaves an already-remote-tracking name unchanged so it is safe to apply twice", () => {
    // Given / When — `ProjectEntry.main_branch_ref` is already `<remote>/<branch>`
    const ref = remoteTrackingName("origin/master");

    // Then
    expect(ref).toBe("origin/master");
  });

  it("prepends the resolved remote when the input carries a foreign remote prefix", () => {
    // Given — the project's remote is `upstream` but the input already carries an `origin/` prefix.
    // `remoteTrackingName` only recognizes the caller's resolved remote, so a foreign-prefixed name
    // is treated as local and lifted under the resolved remote. (Callers in the Start-session dialog
    // pass stack-node local names or a `main_branch_ref` that is already under the resolved remote,
    // so this foreign-prefix case does not arise in practice; the rule stays simple and idempotent
    // for the same remote.)
    // When
    const ref = remoteTrackingName("origin/feature/x", "upstream");

    // Then
    expect(ref).toBe("upstream/origin/feature/x");
  });

  it("reduces an empty branch to an empty ref so an absent base stays absent", () => {
    // Given / When — no base selected; must not become `origin/`
    const ref = remoteTrackingName("");

    // Then
    expect(ref).toBe("");
  });

  it("trims surrounding whitespace before prepending the remote prefix", () => {
    // Given / When
    const ref = remoteTrackingName("  feature/attach-docs/attach-store  ");

    // Then
    expect(ref).toBe("origin/feature/attach-docs/attach-store");
  });
});
