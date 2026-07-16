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
import type { SessionMetadata } from "../lib/sessionParticipantMetadata";
import {
  mergeActiveAndFetchedSessions,
  owningHostForSession,
  parseSessionParticipantIdentity,
  sessionParticipantsFromParticipants,
  type SessionParticipant,
} from "./crossHostSessions";
import { sessionDrawerLabel } from "./sessionDrawerLabel";

const SID_A = "aaaaaaaa-0000-4000-8000-000000000001";
const SID_B = "bbbbbbbb-0000-4000-8000-000000000002";
const SELECTED = "workstation-1";
const OTHER = "server-2";

function aSession(overrides: Partial<SessionEntry>): SessionEntry {
  return create(SessionEntrySchema, { sessionId: "sess-default", daemonInstanceId: "", ...overrides });
}

function aSessionMetadata(overrides: Partial<SessionMetadata> = {}): SessionMetadata {
  return {
    workflowGoal: "Add checkout flow",
    workflowState: "green",
    agent: "claude-cli",
    model: "claude-opus-4-8",
    activityStatus: "Running",
    recipe: "tdd",
    repoPath: "/home/alice/acme-web",
    elapsedDisplay: "3m 20s",
    pendingElicitation: false,
    ...overrides,
  };
}

/** A live cross-host participant carrying the coder-published `session` metadata block. */
function aLiveParticipant(overrides: Partial<SessionParticipant> = {}): SessionParticipant {
  return {
    sessionId: SID_B,
    owningInstanceId: OTHER,
    sessionMetadata: aSessionMetadata(),
    ...overrides,
  };
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

  it("hydrates a synthesized cross-host row with the repo path from participant metadata", () => {
    const participant = aLiveParticipant({
      sessionMetadata: aSessionMetadata({ repoPath: "/home/alice/acme-web" }),
    });

    const merged = mergeActiveAndFetchedSessions([], [participant], SELECTED);

    const synthesized = merged.find((s) => s.sessionId === SID_B);
    expect(synthesized?.repoPath).toBe("/home/alice/acme-web");
  });

  it("hydrates a synthesized cross-host row with the workflow goal and state from participant metadata", () => {
    const participant = aLiveParticipant({
      sessionMetadata: aSessionMetadata({ workflowGoal: "Add checkout flow", workflowState: "green" }),
    });

    const merged = mergeActiveAndFetchedSessions([], [participant], SELECTED);

    const synthesized = merged.find((s) => s.sessionId === SID_B);
    expect(synthesized?.workflowGoal).toBe("Add checkout flow");
    expect(synthesized?.workflowState).toBe("green");
  });

  it("hydrates a synthesized cross-host row with the agent and model from participant metadata", () => {
    const participant = aLiveParticipant({
      sessionMetadata: aSessionMetadata({ agent: "claude-cli", model: "claude-opus-4-8" }),
    });

    const merged = mergeActiveAndFetchedSessions([], [participant], SELECTED);

    const synthesized = merged.find((s) => s.sessionId === SID_B);
    expect(synthesized?.agent).toBe("claude-cli");
    expect(synthesized?.model).toBe("claude-opus-4-8");
  });

  it("hydrates a synthesized cross-host row with the activity status from participant metadata", () => {
    const participant = aLiveParticipant({
      sessionMetadata: aSessionMetadata({ activityStatus: "Running" }),
    });

    const merged = mergeActiveAndFetchedSessions([], [participant], SELECTED);

    const synthesized = merged.find((s) => s.sessionId === SID_B);
    expect(synthesized?.activityStatus).toBe("Running");
  });

  it("keeps the short-session-id fallback when a live participant carries no metadata", () => {
    const participant = aLiveParticipant({ sessionMetadata: undefined });

    const merged = mergeActiveAndFetchedSessions([], [participant], SELECTED);

    const synthesized = merged.find((s) => s.sessionId === SID_B);
    expect(synthesized?.repoPath).toBe("");
    expect(synthesized?.workflowGoal).toBe("");
  });
});

describe("cross-host claimed session drawer label", () => {
  it("shows the repo basename as the drawer name for a claimed session on another screen", () => {
    const participant = aLiveParticipant({
      sessionMetadata: aSessionMetadata({ repoPath: "/home/alice/acme-web", workflowGoal: "Add checkout flow" }),
    });

    const merged = mergeActiveAndFetchedSessions([], [participant], SELECTED);

    const synthesized = merged.find((s) => s.sessionId === SID_B)!;
    expect(sessionDrawerLabel(synthesized)).toBe("acme-web");
  });

  it("falls back to the workflow goal when the claimed session has no repo path", () => {
    const participant = aLiveParticipant({
      sessionMetadata: aSessionMetadata({ repoPath: "", workflowGoal: "Add checkout flow" }),
    });

    const merged = mergeActiveAndFetchedSessions([], [participant], SELECTED);

    const synthesized = merged.find((s) => s.sessionId === SID_B)!;
    expect(sessionDrawerLabel(synthesized)).toBe("Add checkout flow");
  });
});
