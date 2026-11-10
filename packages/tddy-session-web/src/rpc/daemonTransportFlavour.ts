/**
 * Which RPC flavour this page must use to reach its daemon.
 *
 * One web bundle serves two hosts. Inside the Tauri desktop application the daemon runs in the
 * same process and is reached over the host application's IPC bridge — there is no HTTP origin to
 * post to. In a browser the daemon is an HTTP server and the bundle came from it. The page can
 * tell which it is by whether the host application injected its IPC bridge.
 */

/** The part of `window` this decision depends on. */
export interface TauriHostWindow {
  /** Injected by the Tauri host application into every page it loads. */
  __TAURI_INTERNALS__?: unknown;
}

export type TransportFlavour = "webview-ipc" | "http";

/** The flavour `win` must use to reach its daemon. */
export function daemonTransportFlavour(win: TauriHostWindow): TransportFlavour {
  return win.__TAURI_INTERNALS__ === undefined ? "http" : "webview-ipc";
}
