/**
 * Unit tests for `SessionManager` session-metadata overlay — a LiveKit participant's `session`
 * metadata is parsed and surfaced for synthesized cross-host rows and live-updated fetched rows.
 *
 * Changeset: `2026-07-12-fast-session-change`
 * PRD: `docs/ft/web/1-WIP/PRD-2026-07-12-fast-session-change.md` (req 4)
 *
 * ⚠️ RED PHASE — fails until `SessionManager` exposes `sessionMetadataBySessionId` and
 * `SessionParticipant` carries an optional parsed `sessionMetadata` block.
 */

import { describe, it, expect } from "bun:test";
import { create } from "@bufbuild/protobuf";
import { SessionEntrySchema, type SessionEntry } from "../../gen/connection_pb";
import { SessionManager } from "./sessionManager";
import type { SessionParticipant } from "../../utils/crossHostSessions";
import type { SessionMetadata } from "../../lib/sessionParticipantMetadata";

const SELECTED = "workstation-1";
const OTHER = "server-2";
const SID_ON_OTHER = "bbbbbbbb-0000-4000-8000-000000000002";

function aSession(overrides: Partial<SessionEntry>): SessionEntry {
  return create(SessionEntrySchema, { sessionId: "sess", daemonInstanceId: "", ...overrides });
}

function aSessionMetadata(overrides: Partial<SessionMetadata> = {}): SessionMetadata {
  return {
    workflowGoal: "acceptance-tests",
    workflowState: "Red",
    agent: "claude",
    model: "sonnet-4",
    activityStatus: "",
    recipe: "tdd",
    repoPath: "/home/dev/peer-feature",
    elapsedDisplay: "3m",
    pendingElicitation: false,
    ...overrides,
  };
}

function aLiveParticipant(
  sessionId: string,
  owningInstanceId: string,
  metadata?: SessionMetadata,
): SessionParticipant {
  return { sessionId, owningInstanceId, sessionMetadata: metadata };
}

describe("SessionManager — participant session metadata overlay", () => {
  it("surfaces parsed session metadata for a synthesized cross-host row with no fetched entry", async () => {
    // Given — the selected host returns nothing; the peer session is only a live participant
    const manager = new SessionManager();
    manager.setSelectedInstanceId(SELECTED);
    manager.setFetcher(async () => []);
    await Promise.resolve();

    // When — the participant carries a `session` metadata block
    manager.setActiveParticipants([
      aLiveParticipant(SID_ON_OTHER, OTHER, aSessionMetadata({ workflowState: "Green" })),
    ]);

    // Then — the synthesized row exists and its metadata is overlaid
    const byId = new Map(manager.sessions.map((s) => [s.sessionId, s]));
    expect(byId.get(SID_ON_OTHER)?.isActive).toBe(true);
    const meta = manager.sessionMetadataBySessionId.get(SID_ON_OTHER);
    expect(meta?.workflowGoal).toBe("acceptance-tests");
    expect(meta?.workflowState).toBe("Green");
    expect(meta?.agent).toBe("claude");
    expect(meta?.model).toBe("sonnet-4");
  });

  it("live-updates a fetched row's metadata when the participant publishes a new state", async () => {
    // Given — a fetched row on the selected host with a live participant in state Red
    const manager = new SessionManager();
    manager.setSelectedInstanceId(SELECTED);
    manager.setFetcher(async () => [aSession({ sessionId: SID_ON_OTHER, daemonInstanceId: SELECTED })]);
    await Promise.resolve();
    manager.setActiveParticipants([aLiveParticipant(SID_ON_OTHER, SELECTED, aSessionMetadata())]);
    expect(manager.sessionMetadataBySessionId.get(SID_ON_OTHER)?.workflowState).toBe("Red");

    // When — the participant transitions to Green
    manager.setActiveParticipants([
      aLiveParticipant(SID_ON_OTHER, SELECTED, aSessionMetadata({ workflowState: "Green" })),
    ]);

    // Then — the overlaid metadata updates without a new ListSessions fetch
    expect(manager.sessionMetadataBySessionId.get(SID_ON_OTHER)?.workflowState).toBe("Green");
  });

  it("drops a row's overlay when its participant leaves the room", async () => {
    // Given — a synthesized row with metadata
    const manager = new SessionManager();
    manager.setSelectedInstanceId(SELECTED);
    manager.setFetcher(async () => []);
    await Promise.resolve();
    manager.setActiveParticipants([aLiveParticipant(SID_ON_OTHER, OTHER, aSessionMetadata())]);
    expect(manager.sessionMetadataBySessionId.has(SID_ON_OTHER)).toBe(true);

    // When — the participant leaves
    manager.setActiveParticipants([]);

    // Then — the row and its overlay are gone
    expect(manager.sessions.map((s) => s.sessionId)).not.toContain(SID_ON_OTHER);
    expect(manager.sessionMetadataBySessionId.has(SID_ON_OTHER)).toBe(false);
  });
});
