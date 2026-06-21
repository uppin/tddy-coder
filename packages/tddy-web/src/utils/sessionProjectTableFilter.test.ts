import { describe, expect, it } from "bun:test";
import { aProjectEntry, anActiveSession } from "../test-utils";
import { isSessionOrphan, sortedSessionsForProjectTable } from "./sessionProjectTable";

describe("sessions listed under a project accordion", () => {
  it("includes an unscoped session whose repo_path matches the project main_repo_path", () => {
    // Given
    const mainRepo = "/var/tddy/Code/tddy-coder";
    const project = aProjectEntry({
      projectId: "20f8b2d2-1890-49e4-97c0-9572da42a2c5",
      name: "tddy-coder",
      gitUrl: "https://github.com/uppin/tddy-coder.git",
      mainRepoPath: mainRepo,
    });
    const session = anActiveSession({
      sessionId: "019d3d0c-96bb-7941-bc79-777602343e42",
      createdAt: "2026-03-30T04:42:08Z",
      repoPath: mainRepo,
      projectId: "",
    });

    // When
    const rows = sortedSessionsForProjectTable([session], project, [project]);

    // Then
    expect(rows.map((r) => r.sessionId)).toEqual([session.sessionId]);
  });

  it("includes an unscoped session whose repo_path is a git worktree under the main_repo_path", () => {
    // Given
    const mainRepo = "/var/tddy/Code/tddy-coder";
    const project = aProjectEntry({
      projectId: "20f8b2d2-1890-49e4-97c0-9572da42a2c5",
      name: "tddy-coder",
      gitUrl: "https://github.com/uppin/tddy-coder.git",
      mainRepoPath: mainRepo,
    });
    const session = anActiveSession({
      sessionId: "019d3936-1831-7471-b3fd-8b56e324ca5c",
      createdAt: "2026-03-29T10:48:59Z",
      repoPath: `${mainRepo}/.worktrees/feature-branch`,
      projectId: "",
    });

    // When
    const rows = sortedSessionsForProjectTable([session], project, [project]);

    // Then
    expect(rows.map((r) => r.sessionId)).toEqual([session.sessionId]);
  });

  it("assigns an unscoped worktree session to the project with the longest matching main_repo_path", () => {
    // Given — parent project has a shorter path, child project is the more specific match
    const mainRepo = "/var/tddy/Code/tddy-coder";
    const parent = aProjectEntry({
      projectId: "parent",
      name: "parent",
      gitUrl: "https://example.com/p.git",
      mainRepoPath: "/var/tddy/Code",
    });
    const child = aProjectEntry({
      projectId: "child",
      name: "child",
      gitUrl: "https://github.com/uppin/tddy-coder.git",
      mainRepoPath: mainRepo,
    });
    const session = anActiveSession({
      sessionId: "s1",
      createdAt: "2026-03-29T10:48:59Z",
      repoPath: `${mainRepo}/.worktrees/wt`,
      projectId: "",
    });

    // When
    const rowsForChild = sortedSessionsForProjectTable([session], child, [parent, child]);
    const rowsForParent = sortedSessionsForProjectTable([session], parent, [parent, child]);

    // Then — session goes to child (longest prefix), parent gets nothing
    expect(rowsForChild.map((r) => r.sessionId)).toEqual([session.sessionId]);
    expect(rowsForParent).toEqual([]);
  });

  it("does not classify repo-path-matched unscoped sessions as orphans", () => {
    // Given
    const mainRepo = "/var/tddy/Code/tddy-coder";
    const project = aProjectEntry({
      projectId: "20f8b2d2-1890-49e4-97c0-9572da42a2c5",
      name: "tddy-coder",
      gitUrl: "https://github.com/uppin/tddy-coder.git",
      mainRepoPath: mainRepo,
    });
    const session = anActiveSession({
      sessionId: "019d3d0c-96bb-7941-bc79-777602343e42",
      repoPath: mainRepo,
      projectId: "",
    });

    // When / Then
    expect(isSessionOrphan(session, [project])).toBe(false);
  });
});
