/**
 * Unit tests for the cross-host sessions helpers: parsing session coder participants out of the
 * common room, owning-host attribution, and the union of selected-host + live cross-host sessions.
 *
 * Changeset: `show-active-sessions-across-hosts`
 * PRD: `docs/ft/web/session-drawer.md` § Cross-Host Active Sessions
 */

import { describe, it, expect } from "bun:test";
import { create } from "@bufbuild/protobuf";
import { SessionEntrySchema, type SessionEntry } from "../gen/connection_pb";
import {
  mergeActiveAndFetchedSessions,
  owningHostForSession,
  parseSessionParticipantIdentity,
  sessionParticipantsFromParticipants,
} from "./crossHostSessions";

const SID_A = "aaaaaaaa-0000-4000-8000-000000000001";
const SID_B = "bbbbbbbb-0000-4000-8000-000000000002";
const SELECTED = "workstation-1";
const OTHER = "server-2";

function aSession(overrides: Partial<SessionEntry>): SessionEntry {
  return create(SessionEntrySchema, { sessionId: "sess-default", daemonInstanceId: "", ...overrides });
}

describe("parseSessionParticipantIdentity", () => {
  it("parses a multi-host session identity into session id and owning instance", () => {
    expect(parseSessionParticipantIdentity(`daemon-${OTHER}-${SID_B}`)).toEqual({
      sessionId: SID_B,
      owningInstanceId: OTHER,
    });
  });

  it("parses a single-daemon session identity with an empty owning instance", () => {
    expect(parseSessionParticipantIdentity(`daemon-${SID_A}`)).toEqual({
      sessionId: SID_A,
      owningInstanceId: "",
    });
  });

  it("ignores a daemon RPC identity (no trailing session id)", () => {
    expect(parseSessionParticipantIdentity("daemon-server-2")).toBeNull();
  });

  it("ignores browser and bare-daemon-advertisement identities", () => {
    expect(parseSessionParticipantIdentity("web-user-123")).toBeNull();
    expect(parseSessionParticipantIdentity("server-2")).toBeNull();
  });
});

describe("sessionParticipantsFromParticipants", () => {
  it("keeps only session participants, de-duplicated by session id", () => {
    const participants = [
      { identity: `daemon-${OTHER}-${SID_B}` },
      { identity: "daemon-server-2" }, // RPC identity — excluded
      { identity: "web-abc" }, // browser — excluded
      { identity: `daemon-${OTHER}-${SID_B}` }, // duplicate
    ];
    const sessions = sessionParticipantsFromParticipants(participants);
    expect(sessions).toEqual([{ sessionId: SID_B, owningInstanceId: OTHER }]);
  });
});

describe("owningHostForSession", () => {
  it("uses the session's daemonInstanceId when set", () => {
    expect(owningHostForSession(aSession({ daemonInstanceId: OTHER }), SELECTED)).toBe(OTHER);
  });

  it("falls back to the selected host when daemonInstanceId is empty", () => {
    expect(owningHostForSession(aSession({ daemonInstanceId: "" }), SELECTED)).toBe(SELECTED);
  });
});

describe("mergeActiveAndFetchedSessions", () => {
  it("keeps every selected-host session and adds cross-host live sessions", () => {
    const onSelected = aSession({ sessionId: SID_A, daemonInstanceId: SELECTED, repoPath: "/x/a" });
    const merged = mergeActiveAndFetchedSessions(
      [onSelected],
      [{ sessionId: SID_B, owningInstanceId: OTHER }],
      SELECTED,
    );
    const byId = new Map(merged.map((s) => [s.sessionId, s]));
    expect(byId.get(SID_A)?.repoPath).toBe("/x/a"); // metadata preserved
    expect(byId.get(SID_B)?.daemonInstanceId).toBe(OTHER); // synthesized cross-host row, owner set
    expect(byId.get(SID_B)?.isActive).toBe(true);
    expect(merged).toHaveLength(2);
  });

  it("does not synthesize a row for a live session already returned by the selected host", () => {
    const onSelected = aSession({ sessionId: SID_A, daemonInstanceId: SELECTED, repoPath: "/x/a" });
    const merged = mergeActiveAndFetchedSessions(
      [onSelected],
      [{ sessionId: SID_A, owningInstanceId: SELECTED }],
      SELECTED,
    );
    expect(merged).toHaveLength(1);
    expect(merged[0].repoPath).toBe("/x/a"); // kept the metadata-carrying row, not a synthesized one
  });

  it("does not show an inactive session from another host (no participant, not fetched)", () => {
    const onSelected = aSession({ sessionId: SID_A, daemonInstanceId: SELECTED });
    const merged = mergeActiveAndFetchedSessions([onSelected], [], SELECTED);
    expect(merged.map((s) => s.sessionId)).toEqual([SID_A]);
  });
});
