import { describe, expect, it } from "bun:test";
import { anActiveSession, anInactiveSession } from "../test-utils";
import {
  ACTIVITY_BLINK_WINDOW_MS,
  sessionIndicatorFor,
  type SessionNotificationState,
} from "./sessionIndicator";

/**
 * The four states a drawer row's dot can be in, and the rules that pick between them.
 *
 * PRD: docs/ft/daemon/session-notifications.md (FR4–FR6).
 *
 * This is the whole decision, kept pure: liveness, then attention, then recency-against-the-view.
 * Everything the Cypress specs assert is one of these rows; the 30-second decay is only testable
 * here, because a component test that waited it out would cost 30 seconds against a 10-second
 * per-test ceiling.
 */

const NOW = 1_756_000_000_000;

/** A session with nothing recorded against it — the state before its first notification. */
const NOTHING_RECORDED: SessionNotificationState = {
  lastActivityAtMs: 0,
  attentionAtMs: 0,
  seenAtMs: 0,
};

function stateWith(overrides: Partial<SessionNotificationState>): SessionNotificationState {
  return { ...NOTHING_RECORDED, ...overrides };
}

describe("sessionIndicatorFor — a drawer row's dot", () => {
  // -------------------------------------------------------------------------
  // Liveness decides first
  // -------------------------------------------------------------------------

  it("marks an inactive session as disconnected", () => {
    // Given
    const session = anInactiveSession({ repoPath: "/home/dev/ended-branch" });

    // When
    const indicator = sessionIndicatorFor(session, NOTHING_RECORDED, NOW);

    // Then
    expect(indicator).toBe("disconnected");
  });

  it("keeps an inactive session disconnected however recently it reported activity", () => {
    // Given — the agent died mid-turn, so its last activity is seconds old
    const session = anInactiveSession({ repoPath: "/home/dev/ended-branch" });
    const state = stateWith({ lastActivityAtMs: NOW - 1_000 });

    // When
    const indicator = sessionIndicatorFor(session, state, NOW);

    // Then
    expect(indicator).toBe("disconnected");
  });

  it("keeps an inactive session disconnected even when it was asking for input", () => {
    // Given
    const session = anInactiveSession({ repoPath: "/home/dev/ended-branch" });
    const state = stateWith({ attentionAtMs: NOW - 1_000 });

    // When
    const indicator = sessionIndicatorFor(session, state, NOW);

    // Then — answering it would reach nothing, so the row must not claim otherwise
    expect(indicator).toBe("disconnected");
  });

  // -------------------------------------------------------------------------
  // Nothing outstanding
  // -------------------------------------------------------------------------

  it("marks an active session with no notifications as connected", () => {
    // Given
    const session = anActiveSession({ repoPath: "/home/dev/my-feature-branch" });

    // When
    const indicator = sessionIndicatorFor(session, NOTHING_RECORDED, NOW);

    // Then
    expect(indicator).toBe("connected");
  });

  it("marks an active session the feed has never mentioned as connected", () => {
    // Given — the registry holds no entry for this session at all
    const session = anActiveSession({ repoPath: "/home/dev/my-feature-branch" });

    // When
    const indicator = sessionIndicatorFor(session, undefined, NOW);

    // Then
    expect(indicator).toBe("connected");
  });

  // -------------------------------------------------------------------------
  // Working — activity inside the blink window
  // -------------------------------------------------------------------------

  it("marks a session as working while its activity is inside the blink window", () => {
    // Given
    const session = anActiveSession({ repoPath: "/home/dev/my-feature-branch" });
    const state = stateWith({ lastActivityAtMs: NOW - 1_000 });

    // When
    const indicator = sessionIndicatorFor(session, state, NOW);

    // Then
    expect(indicator).toBe("working");
  });

  it("marks a session as working at the last moment of the blink window", () => {
    // Given
    const session = anActiveSession({ repoPath: "/home/dev/my-feature-branch" });
    const state = stateWith({ lastActivityAtMs: NOW - ACTIVITY_BLINK_WINDOW_MS });

    // When
    const indicator = sessionIndicatorFor(session, state, NOW);

    // Then — the window is inclusive; a boundary that excluded it would drop a frame early
    expect(indicator).toBe("working");
  });

  it("settles a session back to connected once the blink window has passed", () => {
    // Given
    const session = anActiveSession({ repoPath: "/home/dev/my-feature-branch" });
    const state = stateWith({ lastActivityAtMs: NOW - ACTIVITY_BLINK_WINDOW_MS - 1 });

    // When
    const indicator = sessionIndicatorFor(session, state, NOW);

    // Then
    expect(indicator).toBe("connected");
  });

  it("blinks for thirty seconds after the last activity", () => {
    // Given — the window the PRD states, pinned as a number so a change to it is deliberate
    // When / Then
    expect(ACTIVITY_BLINK_WINDOW_MS).toBe(30_000);
  });

  // -------------------------------------------------------------------------
  // Needs input — attention newer than the last view
  // -------------------------------------------------------------------------

  it("marks a session as needing input when an attention notification is newer than the last view", () => {
    // Given
    const session = anActiveSession({ repoPath: "/home/dev/my-feature-branch" });
    const state = stateWith({ attentionAtMs: NOW - 5_000, seenAtMs: NOW - 60_000 });

    // When
    const indicator = sessionIndicatorFor(session, state, NOW);

    // Then
    expect(indicator).toBe("needs-input");
  });

  it("keeps a session needing input however long ago it asked", () => {
    // Given — an hour-old question is still unanswered
    const session = anActiveSession({ repoPath: "/home/dev/my-feature-branch" });
    const state = stateWith({ attentionAtMs: NOW - 3_600_000 });

    // When
    const indicator = sessionIndicatorFor(session, state, NOW);

    // Then — attention does not age out; only viewing clears it
    expect(indicator).toBe("needs-input");
  });

  it("prefers needs-input over working when the agent asked after it was working", () => {
    // Given
    const session = anActiveSession({ repoPath: "/home/dev/my-feature-branch" });
    const state = stateWith({ lastActivityAtMs: NOW - 2_000, attentionAtMs: NOW - 1_000 });

    // When
    const indicator = sessionIndicatorFor(session, state, NOW);

    // Then
    expect(indicator).toBe("needs-input");
  });

  it("prefers needs-input over working when the agent went back to work after asking", () => {
    // Given — activity is the newer of the two, but the question is still unanswered
    const session = anActiveSession({ repoPath: "/home/dev/my-feature-branch" });
    const state = stateWith({ attentionAtMs: NOW - 5_000, lastActivityAtMs: NOW - 1_000 });

    // When
    const indicator = sessionIndicatorFor(session, state, NOW);

    // Then
    expect(indicator).toBe("needs-input");
  });

  // -------------------------------------------------------------------------
  // Viewing clears what viewing can clear
  // -------------------------------------------------------------------------

  it("clears the needs-input dot once the session has been viewed", () => {
    // Given — the operator opened the row after the agent asked
    const session = anActiveSession({ repoPath: "/home/dev/my-feature-branch" });
    const state = stateWith({ attentionAtMs: NOW - 5_000, seenAtMs: NOW - 1_000 });

    // When
    const indicator = sessionIndicatorFor(session, state, NOW);

    // Then
    expect(indicator).toBe("connected");
  });

  it("stops blinking once the session has been viewed", () => {
    // Given — activity inside the window, but seen since
    const session = anActiveSession({ repoPath: "/home/dev/my-feature-branch" });
    const state = stateWith({ lastActivityAtMs: NOW - 5_000, seenAtMs: NOW - 1_000 });

    // When
    const indicator = sessionIndicatorFor(session, state, NOW);

    // Then
    expect(indicator).toBe("connected");
  });

  it("blinks again when activity lands after the session was viewed", () => {
    // Given
    const session = anActiveSession({ repoPath: "/home/dev/my-feature-branch" });
    const state = stateWith({ seenAtMs: NOW - 5_000, lastActivityAtMs: NOW - 1_000 });

    // When
    const indicator = sessionIndicatorFor(session, state, NOW);

    // Then
    expect(indicator).toBe("working");
  });

  it("raises needs-input again when the agent asks after the session was viewed", () => {
    // Given
    const session = anActiveSession({ repoPath: "/home/dev/my-feature-branch" });
    const state = stateWith({ seenAtMs: NOW - 5_000, attentionAtMs: NOW - 1_000 });

    // When
    const indicator = sessionIndicatorFor(session, state, NOW);

    // Then
    expect(indicator).toBe("needs-input");
  });

  // -------------------------------------------------------------------------
  // A pending elicitation is not dismissible by looking at it
  // -------------------------------------------------------------------------

  it("keeps a session with a pending elicitation needing input even after it is viewed", () => {
    // Given — a persisted, still-unanswered gate; the operator has looked at the row
    const session = anActiveSession({
      repoPath: "/home/dev/my-feature-branch",
      pendingElicitation: true,
    });
    const state = stateWith({ seenAtMs: NOW });

    // When
    const indicator = sessionIndicatorFor(session, state, NOW);

    // Then — clearing it would claim the operator had answered something they had not
    expect(indicator).toBe("needs-input");
  });

  it("prefers a pending elicitation over recent activity", () => {
    // Given
    const session = anActiveSession({
      repoPath: "/home/dev/my-feature-branch",
      pendingElicitation: true,
    });
    const state = stateWith({ lastActivityAtMs: NOW - 1_000 });

    // When
    const indicator = sessionIndicatorFor(session, state, NOW);

    // Then
    expect(indicator).toBe("needs-input");
  });
});
