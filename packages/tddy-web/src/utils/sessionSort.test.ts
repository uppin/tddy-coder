import { describe, expect, it } from "bun:test";
import { create } from "@bufbuild/protobuf";
import { SessionEntrySchema } from "../gen/connection_pb";
import { sortSessionsForDisplay } from "./sessionSort";

describe("sortSessionsForDisplay", () => {
  it("orders active before inactive, then by createdAt descending, tie-break by sessionId", () => {
    const wrongOrder = [
      create(SessionEntrySchema, {
        sessionId: "proj-order-inactive-old",
        createdAt: "2026-03-21T07:00:00Z",
        status: "exited",
        repoPath: "/p",
        pid: 0,
        isActive: false,
        projectId: "proj-1",
      }),
      create(SessionEntrySchema, {
        sessionId: "proj-order-active-old",
        createdAt: "2026-03-21T08:00:00Z",
        status: "active",
        repoPath: "/p",
        pid: 2,
        isActive: true,
        projectId: "proj-1",
      }),
      create(SessionEntrySchema, {
        sessionId: "proj-order-inactive-new",
        createdAt: "2026-03-21T11:00:00Z",
        status: "exited",
        repoPath: "/p",
        pid: 0,
        isActive: false,
        projectId: "proj-1",
      }),
      create(SessionEntrySchema, {
        sessionId: "proj-order-active-new",
        createdAt: "2026-03-21T12:00:00Z",
        status: "active",
        repoPath: "/p",
        pid: 1,
        isActive: true,
        projectId: "proj-1",
      }),
    ];
    const sorted = sortSessionsForDisplay(wrongOrder);
    expect(sorted.map((s) => s.sessionId)).toEqual([
      "proj-order-active-new",
      "proj-order-active-old",
      "proj-order-inactive-new",
      "proj-order-inactive-old",
    ]);
  });

  it("uses sessionId as stable tie-breaker when createdAt compares equal", () => {
    const sameTime = "2026-03-21T10:00:00Z";
    const a = create(SessionEntrySchema, {
      sessionId: "session-b",
      createdAt: sameTime,
      status: "exited",
      repoPath: "/p",
      pid: 0,
      isActive: false,
      projectId: "p",
    });
    const b = create(SessionEntrySchema, {
      sessionId: "session-a",
      createdAt: sameTime,
      status: "exited",
      repoPath: "/p",
      pid: 0,
      isActive: false,
      projectId: "p",
    });
    const sorted = sortSessionsForDisplay([a, b]);
    expect(sorted.map((s) => s.sessionId)).toEqual(["session-a", "session-b"]);
  });

  it("orders by sessionId when both createdAt values are unparsable", () => {
    const hi = create(SessionEntrySchema, {
      sessionId: "z-last",
      createdAt: "not-a-date",
      status: "exited",
      repoPath: "/p",
      pid: 0,
      isActive: false,
      projectId: "p",
    });
    const lo = create(SessionEntrySchema, {
      sessionId: "a-first",
      createdAt: "also-invalid",
      status: "exited",
      repoPath: "/p",
      pid: 0,
      isActive: false,
      projectId: "p",
    });
    const sorted = sortSessionsForDisplay([hi, lo]);
    expect(sorted.map((s) => s.sessionId)).toEqual(["a-first", "z-last"]);
  });

  it("uses sessionId when one createdAt is valid and the other is not (no time comparison)", () => {
    const valid = create(SessionEntrySchema, {
      sessionId: "session-zzz",
      createdAt: "2026-03-21T12:00:00Z",
      status: "exited",
      repoPath: "/p",
      pid: 0,
      isActive: false,
      projectId: "p",
    });
    const invalid = create(SessionEntrySchema, {
      sessionId: "session-aaa",
      createdAt: "bogus",
      status: "exited",
      repoPath: "/p",
      pid: 0,
      isActive: false,
      projectId: "p",
    });
    const sorted = sortSessionsForDisplay([valid, invalid]);
    expect(sorted.map((s) => s.sessionId)).toEqual(["session-aaa", "session-zzz"]);
  });

  it("returns empty array for empty input", () => {
    expect(sortSessionsForDisplay([])).toEqual([]);
  });

  it("returns a single session unchanged", () => {
    const one = create(SessionEntrySchema, {
      sessionId: "only",
      createdAt: "2026-03-21T10:00:00Z",
      status: "active",
      repoPath: "/p",
      pid: 1,
      isActive: true,
      projectId: "p",
    });
    const sorted = sortSessionsForDisplay([one]);
    expect(sorted).toHaveLength(1);
    expect(sorted[0]?.sessionId).toBe("only");
  });
});
