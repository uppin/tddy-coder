/**
 * Acceptance tests for transport selection: one web bundle, two hosts.
 *
 * See `docs/dev/1-WIP/2026-09-04-tauri-desktop-single-process.md` for the design this validates.
 */

import { describe, it, expect } from "bun:test";
import { daemonTransportFlavour, type TauriHostWindow } from "./daemonTransportFlavour";

/** A page loaded by the Tauri desktop application, which injects its IPC bridge. */
function aTauriHostedWindow(): TauriHostWindow {
  return { __TAURI_INTERNALS__: { invoke: () => Promise.resolve() } };
}

/** A page served over HTTP by a standalone daemon. */
function aPlainBrowserWindow(): TauriHostWindow {
  return {};
}

describe("daemon transport flavour", () => {
  it("names the webview IPC flavour when the page runs inside the Tauri host", () => {
    // Given a page the desktop application loaded
    const win = aTauriHostedWindow();

    // When the flavour is resolved
    const flavour = daemonTransportFlavour(win);

    // Then the page reaches its daemon over the host application's IPC bridge
    expect(flavour).toEqual("webview-ipc");
  });

  it("names the HTTP flavour when the page runs in a plain browser", () => {
    // Given a page a standalone daemon served over HTTP
    const win = aPlainBrowserWindow();

    // When the flavour is resolved
    const flavour = daemonTransportFlavour(win);

    // Then the page reaches its daemon over the same origin it came from
    expect(flavour).toEqual("http");
  });
});
