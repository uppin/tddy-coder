/**
 * The page's side of many concurrent, independently addressed IPC connections.
 *
 * A page used to open exactly one connection, and `daemonTransport.ts` kept a module-level
 * singleton around it — deliberately, because registering a response channel used to *abandon the
 * previous one*, so a page that opened two would abandon its own first connection and leave every
 * call already issued on it waiting forever.
 *
 * Once connections are addressed, that invariant becomes **one bridge per target** instead of one
 * per page. These tests pin the difference.
 *
 * Changeset: `docs/dev/1-WIP/2026-09-05-optional-livekit-multi-connection-ipc.md`
 */

import { describe, it, expect } from "bun:test";
import { DAEMON_TARGET, sessionTarget, thisPagesIpcHost } from "./transport.js";

const A_SESSION = "session-0001";
const ANOTHER_SESSION = "session-0002";

describe("a page's IPC connections", () => {
  it("opens the daemon connection exactly once, however often it is asked for", () => {
    // Given the page's host-application connections
    const host = thisPagesIpcHost();

    // When the daemon bridge is asked for twice — the provider builds one transport, and
    // `useHttpTransport` builds a fallback for any component outside it
    const first = host.openConnection(DAEMON_TARGET);
    const second = host.openConnection(DAEMON_TARGET);

    // Then it is the same bridge. This is the guarantee the `thisPagesBridge` singleton existed to
    // give, preserved now that a page may legitimately open more than one connection.
    expect(second).toBe(first);
  });

  it("gives each session its own bridge", () => {
    // Given two attached sessions
    const host = thisPagesIpcHost();

    const one = host.openConnection(sessionTarget(A_SESSION));
    const other = host.openConnection(sessionTarget(ANOTHER_SESSION));

    // Then each is independent — the IPC equivalent of a room and a participant per session
    expect(other).not.toBe(one);
  });

  it("keeps a session's bridge distinct from the daemon's", () => {
    // Given a page talking to both
    const host = thisPagesIpcHost();

    const daemon = host.openConnection(DAEMON_TARGET);
    const session = host.openConnection(sessionTarget(A_SESSION));

    // Then addressing is real: a call meant for the session cannot land on the daemon's channel
    expect(session).not.toBe(daemon);
  });

  it("returns the same bridge for the same session asked for twice", () => {
    // Given one session reached from two places in the tree
    const host = thisPagesIpcHost();

    const first = host.openConnection(sessionTarget(A_SESSION));
    const second = host.openConnection(sessionTarget(A_SESSION));

    // Then one connection is held, not two. Two would each register a response channel and the
    // second would displace the first for that target.
    expect(second).toBe(first);
  });

  it("opens a fresh bridge for a session reattached after being released", async () => {
    // Given a session that was attached and then detached
    const host = thisPagesIpcHost();
    const first = host.openConnection(sessionTarget(A_SESSION));
    await first.close();

    // When it is attached again
    const second = host.openConnection(sessionTarget(A_SESSION));

    // Then a new connection is opened rather than the released one handed back — a released bridge
    // has no host-side peer, so every call on it would wait for an answer that cannot arrive
    expect(second).not.toBe(first);
  });
});
