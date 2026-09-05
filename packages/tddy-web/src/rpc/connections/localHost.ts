/**
 * The desktop build's own host: reached over IPC, contributed to the directory, and registered
 * ahead of LiveKit.
 *
 * Everything else in this stack was preparation for this file. Nodes 1–4 gave `tddy-web` a provider
 * registry, a source-merged host directory, one session connection carrying capabilities, and
 * surfaces gated on them; node 6 gave the host application concurrent addressed IPC connections.
 * What was still missing is the one thing `tddy-web` must never contain: knowledge of a particular
 * wire. It arrives here, from the desktop build, through the same registries any provider uses.
 *
 * **`tddy-web` must not import this module.** It is loaded by the desktop entry point, so the
 * browser bundle carries no Tauri dependency and there is no `isDesktop` branch anywhere.
 *
 * PRD: `docs/dev/1-WIP/2026-09-05-optional-livekit-desktop-ipc-host-prd.md`.
 */

import type { ConnectionProvider } from "./types";

/** What the desktop build knows about itself when it registers. */
export interface LocalHostRegistration {
  /**
   * The daemon instance serving this page, from `DaemonConfigService.GetClientConfig`'s
   * `daemonInstanceId` — the same payload a browser reads from `/api/config`.
   *
   * Available **before sign-in**, because the daemon serves that call ungated. That matters: the
   * LiveKit path cannot produce a host until authentication completes, since it needs a presence
   * identity derived from the user's login. The IPC path has no such gate, so the desktop app has a
   * usable host from its first paint.
   */
  readonly daemonInstanceId: string;

  /** How the host is named in the selector. */
  readonly label: string;
}

/**
 * The connection provider for the desktop's own host.
 *
 * Registered **first**, so it wins for the host it claims even when a common room is configured and
 * could also reach that machine. The daemon runs in the same process as the webview host, so
 * reaching it through a media server is a round trip out of the machine and back to a roster already
 * in the binary. Precedence expresses that without a user-facing preference.
 *
 * Its connections advertise `{"rpc"}` and nothing else. The daemon *could* publish media into a
 * LiveKit room, but that would make the desktop's own host quietly require the thing this stack made
 * optional — so the media surfaces are absent, which node 4 already handles.
 */
export function createIpcConnectionProvider(
  registration: LocalHostRegistration,
): ConnectionProvider {
  // TODO(desktop-ipc-host): implement
  throw new Error(
    `createIpcConnectionProvider(${registration.daemonInstanceId}) is not implemented yet`,
  );
}

/**
 * `createLocalHostDirectorySource` is **not here yet**, and deliberately so.
 *
 * It returns a `HostDirectorySource`, a type node 2 (`host-directory`) owns. This branch's PR head
 * is based on `capability-gating`, and node 2's commits reach this worktree only through the local
 * `stack-int/desktop-ipc-host` integration ref — so importing that type here would leave this PR's
 * own head unable to compile. It lands under `/green`, in a worktree that has the integration ref or
 * once `host-directory` has merged. Everything below stands alone.
 *
 * The behaviour it will have, recorded now so it is not re-derived later: contribute exactly the
 * serving daemon from `daemonInstanceId`, always `connected` (there is nothing to connect to — the
 * daemon is in this process), labelled with its own `sourceId` so the directory merge prefers it
 * over a common-room advertisement of the same machine.
 */

/**
 * Whether LiveKit should be brought up at all, from the configuration the daemon served.
 *
 * The exact definition of "if settings are configured": both a URL and a common room, non-blank.
 * With either missing the LiveKit source contributes nothing, constructs no `Room`, and calls no
 * `TokenService` — and reports `idle`, not `error`, because an operator who deliberately did not
 * configure LiveKit must not be shown a connection failure for it on every screen.
 */
export function liveKitIsConfigured(config: {
  livekitUrl?: string;
  commonRoom?: string;
}): boolean {
  // TODO(desktop-ipc-host): implement
  throw new Error("liveKitIsConfigured is not implemented yet");
}
