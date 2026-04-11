import { describe, expect, it } from "bun:test";
import { create } from "@bufbuild/protobuf";
import { ProjectEntrySchema, SessionEntrySchema } from "../gen/connection_pb";
import { sortedSessionsForProjectHostTable } from "./sessionProjectTable";

/** Shared logical project id registered on two daemons (multi-daemon collision). */
const SHARED_PROJECT_ID = "aaaaaaaa-bbbb-4ccc-8ddd-eeeeeeeeeeee";
const HOST_WORKSTATION = "workstation-1";
const HOST_SERVER = "server-2";

describe("multi-daemon session grouping (acceptance — PRD Testing Plan)", () => {
  it("groups_sessions_by_project_and_daemon_instance", () => {
    const projectWs = create(ProjectEntrySchema, {
      projectId: SHARED_PROJECT_ID,
      name: "app-ws",
      gitUrl: "https://example.com/app.git",
      mainRepoPath: "/home/ws/app",
      daemonInstanceId: HOST_WORKSTATION,
    });
    const projectSrv = create(ProjectEntrySchema, {
      projectId: SHARED_PROJECT_ID,
      name: "app-srv",
      gitUrl: "https://example.com/app.git",
      mainRepoPath: "/srv/app",
      daemonInstanceId: HOST_SERVER,
    });

    const sessionOnWs = create(SessionEntrySchema, {
      sessionId: "sess-on-ws",
      createdAt: "2026-04-01T10:00:00Z",
      status: "active",
      repoPath: "/home/ws/app",
      pid: 1,
      isActive: true,
      projectId: SHARED_PROJECT_ID,
      daemonInstanceId: HOST_WORKSTATION,
    });
    const sessionOnSrv = create(SessionEntrySchema, {
      sessionId: "sess-on-srv",
      createdAt: "2026-04-01T11:00:00Z",
      status: "active",
      repoPath: "/srv/app",
      pid: 2,
      isActive: true,
      projectId: SHARED_PROJECT_ID,
      daemonInstanceId: HOST_SERVER,
    });

    const all = [sessionOnWs, sessionOnSrv];

    const forWs = sortedSessionsForProjectHostTable(all, projectWs, HOST_WORKSTATION, [projectWs]);
    const forSrv = sortedSessionsForProjectHostTable(all, projectSrv, HOST_SERVER, [projectSrv]);

    expect(forWs.map((s) => s.sessionId)).toEqual([sessionOnWs.sessionId]);
    expect(forSrv.map((s) => s.sessionId)).toEqual([sessionOnSrv.sessionId]);

    const wsIds = new Set(forWs.map((s) => s.sessionId));
    const srvIds = new Set(forSrv.map((s) => s.sessionId));
    const intersection = [...wsIds].filter((id) => srvIds.has(id));
    expect(intersection).toEqual([]);
  });

  it("does_not_merge_sessions_from_two_hosts_with_same_project_id", () => {
    const projectWs = create(ProjectEntrySchema, {
      projectId: SHARED_PROJECT_ID,
      name: "app-ws",
      gitUrl: "https://example.com/app.git",
      mainRepoPath: "/home/ws/app",
      daemonInstanceId: HOST_WORKSTATION,
    });
    const projectSrv = create(ProjectEntrySchema, {
      projectId: SHARED_PROJECT_ID,
      name: "app-srv",
      gitUrl: "https://example.com/app.git",
      mainRepoPath: "/srv/app",
      daemonInstanceId: HOST_SERVER,
    });

    const sessionA = create(SessionEntrySchema, {
      sessionId: "session-A",
      createdAt: "2026-04-01T10:00:00Z",
      status: "active",
      repoPath: "/home/ws/app",
      pid: 1,
      isActive: true,
      projectId: SHARED_PROJECT_ID,
      daemonInstanceId: HOST_WORKSTATION,
    });
    const sessionB = create(SessionEntrySchema, {
      sessionId: "session-B",
      createdAt: "2026-04-01T10:00:00Z",
      status: "active",
      repoPath: "/srv/app",
      pid: 2,
      isActive: true,
      projectId: SHARED_PROJECT_ID,
      daemonInstanceId: HOST_SERVER,
    });

    const both = [sessionA, sessionB];

    const onHost1 = sortedSessionsForProjectHostTable(both, projectWs, HOST_WORKSTATION, [projectWs]);
    const onHost2 = sortedSessionsForProjectHostTable(both, projectSrv, HOST_SERVER, [projectSrv]);

    expect(onHost1.map((s) => s.sessionId)).toContain("session-A");
    expect(onHost1.map((s) => s.sessionId)).not.toContain("session-B");
    expect(onHost2.map((s) => s.sessionId)).toContain("session-B");
    expect(onHost2.map((s) => s.sessionId)).not.toContain("session-A");
  });

  it("unscoped_session_attaches_to_project_on_same_host_only", () => {
    const mainPath = "/home/ws/app";
    const projectWs = create(ProjectEntrySchema, {
      projectId: SHARED_PROJECT_ID,
      name: "app-ws",
      gitUrl: "https://example.com/app.git",
      mainRepoPath: mainPath,
      daemonInstanceId: HOST_WORKSTATION,
    });
    const projectSrv = create(ProjectEntrySchema, {
      projectId: SHARED_PROJECT_ID,
      name: "app-srv",
      gitUrl: "https://example.com/app.git",
      mainRepoPath: mainPath,
      daemonInstanceId: HOST_SERVER,
    });

    const unscoped = create(SessionEntrySchema, {
      sessionId: "unscoped-1",
      createdAt: "2026-04-01T12:00:00Z",
      status: "active",
      repoPath: mainPath,
      pid: 3,
      isActive: true,
      projectId: "",
      daemonInstanceId: HOST_WORKSTATION,
    });

    const rowsWs = sortedSessionsForProjectHostTable([unscoped], projectWs, HOST_WORKSTATION, [
      projectWs,
    ]);
    const rowsSrv = sortedSessionsForProjectHostTable([unscoped], projectSrv, HOST_SERVER, [
      projectSrv,
    ]);

    expect(rowsWs.map((r) => r.sessionId)).toEqual([unscoped.sessionId]);
    expect(rowsSrv).toEqual([]);
  });
});
