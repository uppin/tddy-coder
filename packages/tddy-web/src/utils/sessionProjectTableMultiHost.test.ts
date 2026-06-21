import { describe, expect, it } from "bun:test";
import { aProjectEntry, anActiveSession } from "../test-utils";
import { sortedSessionsForProjectHostTable } from "./sessionProjectTable";

/** Shared project id registered on two daemons (multi-daemon collision scenario). */
const SHARED_PROJECT_ID = "aaaaaaaa-bbbb-4ccc-8ddd-eeeeeeeeeeee";
const HOST_WORKSTATION = "workstation-1";
const HOST_SERVER = "server-2";

describe("multi-daemon session grouping", () => {
  it("routes each session to the host that owns it", () => {
    // Given — same project id on two different daemons, each with one session
    const projectWs = aProjectEntry({ projectId: SHARED_PROJECT_ID, name: "app-ws", gitUrl: "https://example.com/app.git", mainRepoPath: "/home/ws/app", daemonInstanceId: HOST_WORKSTATION });
    const projectSrv = aProjectEntry({ projectId: SHARED_PROJECT_ID, name: "app-srv", gitUrl: "https://example.com/app.git", mainRepoPath: "/srv/app", daemonInstanceId: HOST_SERVER });
    const sessionOnWs = anActiveSession({ sessionId: "sess-on-ws", createdAt: "2026-04-01T10:00:00Z", repoPath: "/home/ws/app", pid: 1, projectId: SHARED_PROJECT_ID, daemonInstanceId: HOST_WORKSTATION });
    const sessionOnSrv = anActiveSession({ sessionId: "sess-on-srv", createdAt: "2026-04-01T11:00:00Z", repoPath: "/srv/app", pid: 2, projectId: SHARED_PROJECT_ID, daemonInstanceId: HOST_SERVER });
    const all = [sessionOnWs, sessionOnSrv];

    // When
    const forWs = sortedSessionsForProjectHostTable(all, projectWs, HOST_WORKSTATION, [projectWs]);
    const forSrv = sortedSessionsForProjectHostTable(all, projectSrv, HOST_SERVER, [projectSrv]);

    // Then
    expect(forWs.map((s) => s.sessionId)).toEqual([sessionOnWs.sessionId]);
    expect(forSrv.map((s) => s.sessionId)).toEqual([sessionOnSrv.sessionId]);
  });

  it("keeps workstation sessions out of the server group when project ids collide", () => {
    // Given
    const projectWs = aProjectEntry({ projectId: SHARED_PROJECT_ID, name: "app-ws", gitUrl: "https://example.com/app.git", mainRepoPath: "/home/ws/app", daemonInstanceId: HOST_WORKSTATION });
    const projectSrv = aProjectEntry({ projectId: SHARED_PROJECT_ID, name: "app-srv", gitUrl: "https://example.com/app.git", mainRepoPath: "/srv/app", daemonInstanceId: HOST_SERVER });
    const sessionA = anActiveSession({ sessionId: "session-A", createdAt: "2026-04-01T10:00:00Z", repoPath: "/home/ws/app", pid: 1, projectId: SHARED_PROJECT_ID, daemonInstanceId: HOST_WORKSTATION });
    const sessionB = anActiveSession({ sessionId: "session-B", createdAt: "2026-04-01T10:00:00Z", repoPath: "/srv/app", pid: 2, projectId: SHARED_PROJECT_ID, daemonInstanceId: HOST_SERVER });
    const both = [sessionA, sessionB];

    // When
    const onHost1 = sortedSessionsForProjectHostTable(both, projectWs, HOST_WORKSTATION, [projectWs]);

    // Then — workstation group contains only session-A
    expect(onHost1.map((s) => s.sessionId)).toContain("session-A");
    expect(onHost1.map((s) => s.sessionId)).not.toContain("session-B");
  });

  it("keeps server sessions out of the workstation group when project ids collide", () => {
    // Given
    const projectWs = aProjectEntry({ projectId: SHARED_PROJECT_ID, name: "app-ws", gitUrl: "https://example.com/app.git", mainRepoPath: "/home/ws/app", daemonInstanceId: HOST_WORKSTATION });
    const projectSrv = aProjectEntry({ projectId: SHARED_PROJECT_ID, name: "app-srv", gitUrl: "https://example.com/app.git", mainRepoPath: "/srv/app", daemonInstanceId: HOST_SERVER });
    const sessionA = anActiveSession({ sessionId: "session-A", createdAt: "2026-04-01T10:00:00Z", repoPath: "/home/ws/app", pid: 1, projectId: SHARED_PROJECT_ID, daemonInstanceId: HOST_WORKSTATION });
    const sessionB = anActiveSession({ sessionId: "session-B", createdAt: "2026-04-01T10:00:00Z", repoPath: "/srv/app", pid: 2, projectId: SHARED_PROJECT_ID, daemonInstanceId: HOST_SERVER });
    const both = [sessionA, sessionB];

    // When
    const onHost2 = sortedSessionsForProjectHostTable(both, projectSrv, HOST_SERVER, [projectSrv]);

    // Then — server group contains only session-B
    expect(onHost2.map((s) => s.sessionId)).toContain("session-B");
    expect(onHost2.map((s) => s.sessionId)).not.toContain("session-A");
  });

  it("attaches an unscoped session to the project on the same host", () => {
    // Given — unscoped session running on the workstation daemon
    const mainPath = "/home/ws/app";
    const projectWs = aProjectEntry({ projectId: SHARED_PROJECT_ID, name: "app-ws", gitUrl: "https://example.com/app.git", mainRepoPath: mainPath, daemonInstanceId: HOST_WORKSTATION });
    const projectSrv = aProjectEntry({ projectId: SHARED_PROJECT_ID, name: "app-srv", gitUrl: "https://example.com/app.git", mainRepoPath: mainPath, daemonInstanceId: HOST_SERVER });
    const unscoped = anActiveSession({ sessionId: "unscoped-1", createdAt: "2026-04-01T12:00:00Z", repoPath: mainPath, pid: 3, projectId: "", daemonInstanceId: HOST_WORKSTATION });

    // When
    const rowsWs = sortedSessionsForProjectHostTable([unscoped], projectWs, HOST_WORKSTATION, [projectWs]);
    const rowsSrv = sortedSessionsForProjectHostTable([unscoped], projectSrv, HOST_SERVER, [projectSrv]);

    // Then — belongs to workstation, not server
    expect(rowsWs.map((r) => r.sessionId)).toEqual([unscoped.sessionId]);
    expect(rowsSrv).toEqual([]);
  });
});
