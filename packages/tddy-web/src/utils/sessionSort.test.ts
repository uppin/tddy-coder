import { describe, expect, it } from "bun:test";
import { anActiveSession, anInactiveSession } from "../test-utils";
import { sortSessionsForDisplay, sortSessionsByCreation } from "./sessionSort";

describe("sortSessionsForDisplay", () => {
  it("places active sessions before inactive, then orders by createdAt descending", () => {
    // Given — sessions in deliberately wrong order (inactive first, oldest active last)
    const sessions = [
      anInactiveSession({ sessionId: "proj-order-inactive-old", createdAt: "2026-03-21T07:00:00Z", repoPath: "/p", projectId: "proj-1" }),
      anActiveSession({ sessionId: "proj-order-active-old", createdAt: "2026-03-21T08:00:00Z", pid: 2, repoPath: "/p", projectId: "proj-1" }),
      anInactiveSession({ sessionId: "proj-order-inactive-new", createdAt: "2026-03-21T11:00:00Z", repoPath: "/p", projectId: "proj-1" }),
      anActiveSession({ sessionId: "proj-order-active-new", createdAt: "2026-03-21T12:00:00Z", pid: 1, repoPath: "/p", projectId: "proj-1" }),
    ];

    // When
    const sorted = sortSessionsForDisplay(sessions);

    // Then
    expect(sorted).toContainSessionIdsInOrder([
      "proj-order-active-new",
      "proj-order-active-old",
      "proj-order-inactive-new",
      "proj-order-inactive-old",
    ]);
  });

  it("uses sessionId as a stable tie-breaker when createdAt timestamps are equal", () => {
    // Given — two inactive sessions with the same timestamp
    const sameTime = "2026-03-21T10:00:00Z";
    const sessions = [
      anInactiveSession({ sessionId: "session-b", createdAt: sameTime, repoPath: "/p", projectId: "p" }),
      anInactiveSession({ sessionId: "session-a", createdAt: sameTime, repoPath: "/p", projectId: "p" }),
    ];

    // When
    const sorted = sortSessionsForDisplay(sessions);

    // Then
    expect(sorted).toContainSessionIdsInOrder(["session-a", "session-b"]);
  });

  it("falls back to sessionId order when both createdAt values are unparsable", () => {
    // Given
    const sessions = [
      anInactiveSession({ sessionId: "z-last", createdAt: "not-a-date", repoPath: "/p", projectId: "p" }),
      anInactiveSession({ sessionId: "a-first", createdAt: "also-invalid", repoPath: "/p", projectId: "p" }),
    ];

    // When
    const sorted = sortSessionsForDisplay(sessions);

    // Then
    expect(sorted).toContainSessionIdsInOrder(["a-first", "z-last"]);
  });

  it("falls back to sessionId order when one createdAt is valid and the other is not", () => {
    // Given
    const sessions = [
      anInactiveSession({ sessionId: "session-zzz", createdAt: "2026-03-21T12:00:00Z", repoPath: "/p", projectId: "p" }),
      anInactiveSession({ sessionId: "session-aaa", createdAt: "bogus", repoPath: "/p", projectId: "p" }),
    ];

    // When
    const sorted = sortSessionsForDisplay(sessions);

    // Then
    expect(sorted).toContainSessionIdsInOrder(["session-aaa", "session-zzz"]);
  });

  it("returns an empty array for empty input", () => {
    // When / Then
    expect(sortSessionsForDisplay([])).toEqual([]);
  });

  it("returns a single session unchanged", () => {
    // Given
    const only = anActiveSession({ sessionId: "only", repoPath: "/p", projectId: "p" });

    // When
    const sorted = sortSessionsForDisplay([only]);

    // Then
    expect(sorted).toContainSessionIdsInOrder(["only"]);
  });
});

describe("sortSessionsByCreation — pure chronological order, newest first", () => {
  it("orders sessions newest-first by createdAt, regardless of isActive status", () => {
    // Given — an old active and a newer inactive: inactive should appear first because it's newer
    const sessions = [
      anActiveSession({ sessionId: "old-active", createdAt: "2026-03-21T08:00:00Z", repoPath: "/p", projectId: "p" }),
      anInactiveSession({ sessionId: "new-inactive", createdAt: "2026-03-21T12:00:00Z", repoPath: "/p", projectId: "p" }),
    ];

    // When
    const sorted = sortSessionsByCreation(sessions);

    // Then
    expect(sorted).toContainSessionIdsInOrder(["new-inactive", "old-active"]);
  });

  it("places the session with the most recent createdAt first across mixed statuses", () => {
    // Given — four sessions spanning a wide time range in random order
    const sessions = [
      anInactiveSession({ sessionId: "sess-b", createdAt: "2026-03-21T09:00:00Z", repoPath: "/p", projectId: "p" }),
      anActiveSession({ sessionId: "sess-d", createdAt: "2026-03-21T07:00:00Z", repoPath: "/p", projectId: "p" }),
      anActiveSession({ sessionId: "sess-a", createdAt: "2026-03-21T12:00:00Z", repoPath: "/p", projectId: "p" }),
      anInactiveSession({ sessionId: "sess-c", createdAt: "2026-03-21T08:00:00Z", repoPath: "/p", projectId: "p" }),
    ];

    // When
    const sorted = sortSessionsByCreation(sessions);

    // Then
    expect(sorted).toContainSessionIdsInOrder(["sess-a", "sess-b", "sess-c", "sess-d"]);
  });

  it("uses sessionId as a stable tie-breaker when createdAt timestamps are equal", () => {
    // Given — two sessions with identical timestamps
    const sameTime = "2026-03-21T10:00:00Z";
    const sessions = [
      anActiveSession({ sessionId: "z-second", createdAt: sameTime, repoPath: "/p", projectId: "p" }),
      anActiveSession({ sessionId: "a-first", createdAt: sameTime, repoPath: "/p", projectId: "p" }),
    ];

    // When
    const sorted = sortSessionsByCreation(sessions);

    // Then
    expect(sorted).toContainSessionIdsInOrder(["a-first", "z-second"]);
  });

  it("falls back to sessionId order when createdAt values are unparsable", () => {
    // Given
    const sessions = [
      anInactiveSession({ sessionId: "z-last", createdAt: "not-a-date", repoPath: "/p", projectId: "p" }),
      anInactiveSession({ sessionId: "a-first", createdAt: "also-invalid", repoPath: "/p", projectId: "p" }),
    ];

    // When
    const sorted = sortSessionsByCreation(sessions);

    // Then
    expect(sorted).toContainSessionIdsInOrder(["a-first", "z-last"]);
  });

  it("returns an empty array for empty input", () => {
    // When / Then
    expect(sortSessionsByCreation([])).toEqual([]);
  });

  it("returns a single session unchanged", () => {
    // Given
    const only = anActiveSession({ sessionId: "only-one", repoPath: "/p", projectId: "p" });

    // When
    const sorted = sortSessionsByCreation([only]);

    // Then
    expect(sorted).toContainSessionIdsInOrder(["only-one"]);
  });

  it("does not mutate the original array", () => {
    // Given
    const sessions = [
      anActiveSession({ sessionId: "b-later", createdAt: "2026-03-21T09:00:00Z", repoPath: "/p", projectId: "p" }),
      anActiveSession({ sessionId: "a-earlier", createdAt: "2026-03-21T12:00:00Z", repoPath: "/p", projectId: "p" }),
    ];
    const originalOrder = sessions.map((s) => s.sessionId);

    // When
    sortSessionsByCreation(sessions);

    // Then — original array is unchanged
    expect(sessions.map((s) => s.sessionId)).toEqual(originalOrder);
  });
});
