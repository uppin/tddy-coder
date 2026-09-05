/**
 * The two hosts one web bundle runs in, as test builders.
 *
 * `daemonTransportFlavour` decides between them from the page's `window`; these produce the two
 * environments it decides on, so a test says which host it is in rather than poking at globals.
 *
 * For bun:test only — never import from Cypress tests.
 */

import type { DescService } from "@bufbuild/protobuf";
import type { Transport } from "@connectrpc/connect";
import type { DaemonHostEnvironment } from "../rpc/daemonTransport";
import { aWebviewIpcHostServing } from "./webviewIpcHost";

/** A page the Tauri desktop application loaded, hosting `service` out of `transport` in-process. */
export function aTauriHostedPage(
  service: DescService,
  transport: Transport,
): DaemonHostEnvironment {
  return {
    window: { __TAURI_INTERNALS__: {} },
    createIpcBridge: () => aWebviewIpcHostServing(service, transport),
  };
}

/**
 * A page a standalone daemon served over HTTP from `origin`. Opening the host application's IPC
 * bridge from here is a bug, not a fallback, so the builder refuses it outright.
 */
export function aBrowserPageServedFrom(origin: string): DaemonHostEnvironment {
  return {
    window: { location: { origin } },
    createIpcBridge: () => {
      throw new Error("a browser page must not open the host application's IPC bridge");
    },
  };
}
