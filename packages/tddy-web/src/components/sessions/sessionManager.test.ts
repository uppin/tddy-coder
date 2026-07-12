/**
 * Unit tests for the `SessionManager` store: merging the selected host's fetched sessions with live
 * cross-host participants, refresh, optimistic entries, and change notifications.
 *
 * Changeset: `show-active-sessions-across-hosts`
 */

import { describe, it, expect } from "bun:test";
import { create } from "@bufbuild/protobuf";
import { SessionEntrySchema, type SessionEntry } from "../../gen/connection_pb";
import { SessionManager } from "./sessionManager";

const SELECTED = "workstation-1";
const OTHER = "server-2";
const SID_ON_SELECTED = "aaaaaaaa-0000-4000-8000-000000000001";
const SID_ON_OTHER = "bbbbbbbb-0000-4000-8000-000000000002";

function aSession(overrides: Partial<SessionEntry>): SessionEntry {
  return create(SessionEntrySchema, { sessionId: "sess", daemonInstanceId: "", ...overrides });
}

function managerFor(fetched: SessionEntry[]) {
  const manager = new SessionManager();
  manager.setSelectedInstanceId(SELECTED);
  manager.setFetcher(async () => fetched);
  return manager;
}

describe("SessionManager", () => {
  it("lists the selected host's sessions once a fetcher is set", async () => {
    const manager = managerFor([aSession({ sessionId: SID_ON_SELECTED, daemonInstanceId: SELECTED })]);
    await Promise.resolve(); // let the fetch settle
    expect(manager.sessions.map((s) => s.sessionId)).toEqual([SID_ON_SELECTED]);
  });

  it("adds a live cross-host session from participants that the selected host never returned", async () => {
    const manager = managerFor([aSession({ sessionId: SID_ON_SELECTED, daemonInstanceId: SELECTED })]);
    await Promise.resolve();
    manager.setActiveParticipants([{ sessionId: SID_ON_OTHER, owningInstanceId: OTHER }]);

    const byId = new Map(manager.sessions.map((s) => [s.sessionId, s]));
    expect(byId.has(SID_ON_SELECTED)).toBe(true);
    expect(byId.get(SID_ON_OTHER)?.daemonInstanceId).toBe(OTHER);
    expect(byId.get(SID_ON_OTHER)?.isActive).toBe(true);
  });

  it("notifies subscribers when the list changes", () => {
    const manager = new SessionManager();
    manager.setSelectedInstanceId(SELECTED);
    let notifications = 0;
    manager.subscribe(() => {
      notifications += 1;
    });
    manager.setActiveParticipants([{ sessionId: SID_ON_OTHER, owningInstanceId: OTHER }]);
    expect(notifications).toBe(1);
    expect(manager.sessions.map((s) => s.sessionId)).toEqual([SID_ON_OTHER]);
  });

  it("re-pulls the selected host's sessions on refresh()", async () => {
    let current: SessionEntry[] = [];
    const manager = new SessionManager();
    manager.setSelectedInstanceId(SELECTED);
    manager.setFetcher(async () => current);
    await Promise.resolve();
    expect(manager.sessions).toHaveLength(0);

    current = [aSession({ sessionId: SID_ON_SELECTED, daemonInstanceId: SELECTED })];
    manager.refresh();
    await Promise.resolve();
    expect(manager.sessions.map((s) => s.sessionId)).toEqual([SID_ON_SELECTED]);
  });

  it("keeps an optimistic session until a refresh, without duplicating a fetched one", async () => {
    const manager = managerFor([]);
    await Promise.resolve();
    manager.addOptimisticSession(aSession({ sessionId: SID_ON_SELECTED, daemonInstanceId: SELECTED }));
    expect(manager.sessions.map((s) => s.sessionId)).toEqual([SID_ON_SELECTED]);

    manager.addOptimisticSession(aSession({ sessionId: SID_ON_SELECTED, daemonInstanceId: SELECTED }));
    expect(manager.sessions).toHaveLength(1); // de-duplicated
  });
});
