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
 * Reference: `packages/tddy-desktop/docs/webview-ipc-connections.md`
 */

import { afterEach, describe, expect, it } from "bun:test";
import {
  DAEMON_TARGET,
  sessionTarget,
  thisPagesIpcHost,
  type ConnectionTarget,
  type WebviewIpcBridge,
} from "./transport.js";

const A_SESSION = "session-0001";
const ANOTHER_SESSION = "session-0002";

/** Every bridge this test opened, so the page can be left holding none of them. */
const attached: WebviewIpcBridge[] = [];

/** The page's bridge to `target`, released when the test that asked for it ends. */
function pagesBridgeTo(target: ConnectionTarget): WebviewIpcBridge {
  const bridge = thisPagesIpcHost().openConnection(target);
  attached.push(bridge);
  return bridge;
}

afterEach(async () => {
  // The registry belongs to the page, not to a test, so a connection left open is still open for
  // whoever runs next — and every "Given the page holds..." below would be describing a page some
  // earlier test had already furnished. Releasing through `close` is how a detach empties it in
  // production, so isolation costs no test-only door into the registry.
  await Promise.all(attached.splice(0).map((bridge) => bridge.close()));
});

describe("a page's IPC connections", () => {
  it("opens the daemon connection exactly once, however often it is asked for", () => {
    // Given a page holding no connections yet

    // When its daemon bridge is asked for twice — the provider builds one transport, and
    // `useHttpTransport` builds a fallback for any component outside it
    const first = pagesBridgeTo(DAEMON_TARGET);
    const second = pagesBridgeTo(DAEMON_TARGET);

    // Then it is the same bridge. This is the guarantee the `thisPagesBridge` singleton existed to
    // give, preserved now that a page may legitimately open more than one connection.
    expect(second).toBe(first);
  });

  it("gives each session its own bridge", () => {
    // Given two attached sessions
    const one = pagesBridgeTo(sessionTarget(A_SESSION));
    const other = pagesBridgeTo(sessionTarget(ANOTHER_SESSION));

    // Then each is independent — the IPC equivalent of a room and a participant per session
    expect(other).not.toBe(one);
  });

  it("keeps a session's bridge distinct from the daemon's", () => {
    // Given a page talking to both
    const daemon = pagesBridgeTo(DAEMON_TARGET);
    const session = pagesBridgeTo(sessionTarget(A_SESSION));

    // Then addressing is real: a call meant for the session cannot land on the daemon's channel
    expect(session).not.toBe(daemon);
  });

  it("returns the same bridge for the same session asked for twice", () => {
    // Given one session reached from two places in the tree
    const first = pagesBridgeTo(sessionTarget(A_SESSION));
    const second = pagesBridgeTo(sessionTarget(A_SESSION));

    // Then one connection is held, not two. Two would each register a response channel and the
    // second would displace the first for that target.
    expect(second).toBe(first);
  });

  it("opens a fresh bridge for a session reattached after being released", async () => {
    // Given a session that was attached and then detached
    const first = pagesBridgeTo(sessionTarget(A_SESSION));
    await first.close();

    // When it is attached again
    const second = pagesBridgeTo(sessionTarget(A_SESSION));

    // Then a new connection is opened rather than the released one handed back — a released bridge
    // has no host-side peer, so every call on it would wait for an answer that cannot arrive
    expect(second).not.toBe(first);
  });

  it("gives two targets' bridges different epochs", () => {
    // Given the page reaching the daemon and a session
    const daemon = pagesBridgeTo(DAEMON_TARGET);
    const session = pagesBridgeTo(sessionTarget(A_SESSION));

    // Then the two connections are told apart by the number their frames carry. The host routes a
    // frame by its epoch alone, so a shared one would answer either connection into whichever of
    // them registered last.
    expect(session.clientEpoch).not.toEqual(daemon.clientEpoch);
  });

  it("keeps one epoch for a session however often its bridge is asked for", () => {
    // Given one session reached from two places in the tree
    const first = pagesBridgeTo(sessionTarget(A_SESSION));
    const second = pagesBridgeTo(sessionTarget(A_SESSION));

    // Then both call sites stamp their frames with the one epoch the session's single response
    // channel is registered under
    expect(second.clientEpoch).toEqual(first.clientEpoch);
  });
});
