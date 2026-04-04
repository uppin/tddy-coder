import { describe, expect, it } from "bun:test";
import { create } from "@bufbuild/protobuf";
import { ProjectEntrySchema, SessionEntrySchema } from "../gen/connection_pb";
import { isSessionOrphan, sortedSessionsForProjectTable } from "./sessionProjectTable";

describe("sessions listed under a project accordion", () => {
  it("includes a session with empty project_id when repo_path equals the project main_repo_path", () => {
    const mainRepo = "/var/tddy/Code/tddy-coder";
    const project = create(ProjectEntrySchema, {
      projectId: "20f8b2d2-1890-49e4-97c0-9572da42a2c5",
      name: "tddy-coder",
      gitUrl: "https://github.com/uppin/tddy-coder.git",
      mainRepoPath: mainRepo,
    });
    const sessionUnscoped = create(SessionEntrySchema, {
      sessionId: "019d3d0c-96bb-7941-bc79-777602343e42",
      createdAt: "2026-03-30T04:42:08Z",
      status: "active",
      repoPath: mainRepo,
      pid: 1,
      isActive: true,
      projectId: "",
    });

    const rows = sortedSessionsForProjectTable([sessionUnscoped], project, [project]);

    expect(rows.map((r) => r.sessionId)).toEqual([sessionUnscoped.sessionId]);
  });

  it("includes unscoped sessions whose repo_path is a git worktree under main_repo_path", () => {
    const mainRepo = "/var/tddy/Code/tddy-coder";
    const project = create(ProjectEntrySchema, {
      projectId: "20f8b2d2-1890-49e4-97c0-9572da42a2c5",
      name: "tddy-coder",
      gitUrl: "https://github.com/uppin/tddy-coder.git",
      mainRepoPath: mainRepo,
    });
    const sessionWorktree = create(SessionEntrySchema, {
      sessionId: "019d3936-1831-7471-b3fd-8b56e324ca5c",
      createdAt: "2026-03-29T10:48:59Z",
      status: "active",
      repoPath: `${mainRepo}/.worktrees/feature-branch`,
      pid: 1,
      isActive: true,
      projectId: "",
    });

    const rows = sortedSessionsForProjectTable([sessionWorktree], project, [project]);

    expect(rows.map((r) => r.sessionId)).toEqual([sessionWorktree.sessionId]);
  });

  it("uses the longest main_repo_path prefix when multiple projects match", () => {
    const mainRepo = "/var/tddy/Code/tddy-coder";
    const parent = create(ProjectEntrySchema, {
      projectId: "parent",
      name: "parent",
      gitUrl: "https://example.com/p.git",
      mainRepoPath: "/var/tddy/Code",
    });
    const child = create(ProjectEntrySchema, {
      projectId: "child",
      name: "child",
      gitUrl: "https://github.com/uppin/tddy-coder.git",
      mainRepoPath: mainRepo,
    });
    const sessionWorktree = create(SessionEntrySchema, {
      sessionId: "s1",
      createdAt: "2026-03-29T10:48:59Z",
      status: "active",
      repoPath: `${mainRepo}/.worktrees/wt`,
      pid: 1,
      isActive: true,
      projectId: "",
    });

    const rowsForChild = sortedSessionsForProjectTable(
      [sessionWorktree],
      child,
      [parent, child]
    );
    const rowsForParent = sortedSessionsForProjectTable(
      [sessionWorktree],
      parent,
      [parent, child]
    );

    expect(rowsForChild.map((r) => r.sessionId)).toEqual([sessionWorktree.sessionId]);
    expect(rowsForParent).toEqual([]);
  });

  it("does not classify repo-matched unscoped sessions as orphans", () => {
    const mainRepo = "/var/tddy/Code/tddy-coder";
    const project = create(ProjectEntrySchema, {
      projectId: "20f8b2d2-1890-49e4-97c0-9572da42a2c5",
      name: "tddy-coder",
      gitUrl: "https://github.com/uppin/tddy-coder.git",
      mainRepoPath: mainRepo,
    });
    const sessionUnscoped = create(SessionEntrySchema, {
      sessionId: "019d3d0c-96bb-7941-bc79-777602343e42",
      createdAt: "2026-03-30T04:42:08Z",
      status: "active",
      repoPath: mainRepo,
      pid: 1,
      isActive: true,
      projectId: "",
    });
    expect(isSessionOrphan(sessionUnscoped, [project])).toBe(false);
  });
});
