/**
 * When a file dropped onto a session terminal is handed over as its real path, rather than
 * uploaded. PRD: docs/ft/web/web-terminal.md § File drop upload.
 */

import { describe, expect, it } from "bun:test";
import type { TauriHostWindow } from "../rpc/daemonTransportFlavour";
import { sessionReadsLocalPaths } from "./sessionReadsLocalPaths";

const THIS_APPS_DAEMON = "daemon-this-mac";
const ANOTHER_DAEMON = "daemon-build-box";

function insideTddyDesktop(): TauriHostWindow {
  return { __TAURI_INTERNALS__: {} };
}

function inABrowser(): TauriHostWindow {
  return {};
}

function aSession(daemonInstanceId: string, readsHostPaths: boolean) {
  return { daemonInstanceId, readsHostPaths };
}

describe("a session reads paths on this machine", () => {
  it("when it runs unconfined on the daemon Tddy Desktop hosts", () => {
    // Given / When
    const reads = sessionReadsLocalPaths(
      insideTddyDesktop(),
      aSession(THIS_APPS_DAEMON, true),
      THIS_APPS_DAEMON,
    );

    // Then
    expect(reads).toBe(true);
  });

  it("not when it runs on another host, however unconfined", () => {
    expect(
      sessionReadsLocalPaths(insideTddyDesktop(), aSession(ANOTHER_DAEMON, true), THIS_APPS_DAEMON),
    ).toBe(false);
  });

  it("not when its agent is confined on this machine", () => {
    expect(
      sessionReadsLocalPaths(
        insideTddyDesktop(),
        aSession(THIS_APPS_DAEMON, false),
        THIS_APPS_DAEMON,
      ),
    ).toBe(false);
  });

  it("not in a browser, where the serving daemon is not this machine", () => {
    expect(
      sessionReadsLocalPaths(inABrowser(), aSession(THIS_APPS_DAEMON, true), THIS_APPS_DAEMON),
    ).toBe(false);
  });

  it("not when the session or the application's own daemon is unknown", () => {
    expect(sessionReadsLocalPaths(insideTddyDesktop(), undefined, THIS_APPS_DAEMON)).toBe(false);
    expect(
      sessionReadsLocalPaths(insideTddyDesktop(), aSession(THIS_APPS_DAEMON, true), undefined),
    ).toBe(false);
    expect(sessionReadsLocalPaths(insideTddyDesktop(), aSession("", true), "")).toBe(false);
  });
});
