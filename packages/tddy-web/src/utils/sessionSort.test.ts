import { describe, expect, it } from "bun:test";
import { anActiveSession, anInactiveSession } from "../test-utils";
import { sortSessionsForDisplay } from "./sessionSort";

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
