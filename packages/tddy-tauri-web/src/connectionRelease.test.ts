/**
 * Releasing a connection, as the host application sees it.
 *
 * A page that detaches a session and keeps quiet about it leaves the host holding that session's
 * peer for as long as the page lives: the forwards keep publishing, the sink stays open, and the
 * next attach adds another. So the release is not an internal tidy-up — it is a command the page
 * owes the host, and these tests watch that command rather than anything on the page side of it.
 *
 * Reference: `packages/tddy-desktop/docs/webview-ipc-connections.md`
 */

import { beforeEach, describe, expect, it } from "bun:test";
import { recordedHostApplicationCommands } from "./test-utils/hostApplicationCommands.js";

// Before the module under test is loaded, so the bridges it builds invoke the recorded surface
// rather than a real host application no test is running inside.
const hostApplication = recordedHostApplicationCommands();
const { sessionTarget, thisPagesIpcHost } = await import("./transport.js");

const A_SESSION = "session-being-released";
const A_SESSION_NEVER_USED = "session-never-called-on";

beforeEach(() => {
  hostApplication.forgetRecorded();
});

describe("releasing a page's connection", () => {
  it("asks the host to forget the connection the page is giving up", async () => {
    // Given an attached session whose response channel the page has registered
    const bridge = thisPagesIpcHost().openConnection(sessionTarget(A_SESSION));
    await bridge.connect(() => {});

    // When the session is detached
    await bridge.close();

    // Then the host is told which peer to drop, by the epoch that peer registered under — the one
    // handle it has to the state, forwards and sink held for this connection
    expect(hostApplication.releasedEpochs()).toEqual([bridge.clientEpoch]);
  });

  it("asks the host for nothing when the connection it releases was never registered", async () => {
    // Given a bridge the page opened but never called on — a component that mounted and unmounted
    // without issuing a request
    const bridge = thisPagesIpcHost().openConnection(sessionTarget(A_SESSION_NEVER_USED));

    // When it is released
    await bridge.close();

    // Then the host is not asked to forget a peer it was never asked to hold. An epoch it has no
    // connection for is one it can only reject.
    expect(hostApplication.invoked()).toEqual([]);
  });
});
