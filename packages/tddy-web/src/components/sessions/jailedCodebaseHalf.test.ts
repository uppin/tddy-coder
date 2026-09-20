/**
 * Unit tests for `withoutJailedCodebaseHalves` — the drawer's rule for showing one row per session
 * on a placement that is served by two.
 *
 * A `sandboxed_codebase` session is two sessions on ONE daemon: the `claude-cli` agent, and the
 * `workspace` session whose jail holds the checkout. Both are returned by `ListSessions`, so the
 * drawer listed a session the operator never started and cannot use — see
 * docs/ft/daemon/remote-managed-worktree.md § Sandboxed codebase placement, whose sessions list
 * renders one row carrying both roles.
 */

import { describe, it, expect } from "bun:test";
import { create } from "@bufbuild/protobuf";
import { SessionEntrySchema, type SessionEntry } from "../../gen/session_pb";

import { withoutJailedCodebaseHalves } from "./jailedCodebaseHalf";

const AGENT = "01a0bafe-cc07-7e83-8a9a-411854a29caa";
const CODEBASE = "01a0bafe-cc08-7e33-9fb3-d99bc95d75ae";

function aSession(overrides: Partial<SessionEntry>): SessionEntry {
  return create(SessionEntrySchema, { sessionId: "sess", daemonInstanceId: "", ...overrides });
}

function ids(sessions: SessionEntry[]): string[] {
  return sessions.map((s) => s.sessionId);
}

describe("withoutJailedCodebaseHalves", () => {
  it("drops the codebase half a sandboxed-codebase agent names, keeping the agent row", () => {
    // Given — the two sessions one sandboxed-codebase placement is served by
    const agent = aSession({ sessionId: AGENT, codebaseSessionId: CODEBASE });
    const codebase = aSession({ sessionId: CODEBASE, sessionType: "workspace" });

    // When — the drawer shapes the list
    const shown = withoutJailedCodebaseHalves([agent, codebase]);

    // Then — one row, the agent's
    expect(ids(shown)).toEqual([AGENT]);
  });

  it("keeps a workspace session no agent claims, which is a session of its own", () => {
    // Given — a standalone workspace session, named by nobody
    const standalone = aSession({ sessionId: "ws-1", sessionType: "workspace" });
    const other = aSession({ sessionId: "agent-1" });

    // When
    const shown = withoutJailedCodebaseHalves([standalone, other]);

    // Then — both stay: only a claimed half is an implementation detail
    expect(ids(shown)).toEqual(["ws-1", "agent-1"]);
  });

  it("leaves an ordinary co-located list untouched", () => {
    // Given — sessions that name no codebase half at all
    const list = [aSession({ sessionId: "a" }), aSession({ sessionId: "b" })];

    // When / Then
    expect(ids(withoutJailedCodebaseHalves(list))).toEqual(["a", "b"]);
  });

  it("keeps the claimed half when its agent is not in the list, so it cannot vanish entirely", () => {
    // Given — only the codebase half is listed (its agent is filtered out by host, say)
    const codebase = aSession({ sessionId: CODEBASE, sessionType: "workspace" });

    // When / Then — nothing claims it here, so it stays reachable rather than disappearing
    expect(ids(withoutJailedCodebaseHalves([codebase]))).toEqual([CODEBASE]);
  });
});
