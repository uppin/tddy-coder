/**
 * Presence, as an explicitly requested capability rather than an ambient `Room`.
 *
 * `SelectedDaemonContextValue` used to publish `room: Room | null` to the whole subtree, so any
 * component could reach LiveKit without declaring that it needed presence — and the capability
 * gating in node 4 would have had no seam to gate on. This is that seam: a component that wants the
 * participant roster asks for it by name, and gets `null` on a connection that has none.
 *
 * PRD: `docs/dev/1-WIP/2026-09-05-optional-livekit-host-directory-prd.md`.
 */

import type { Room } from "livekit-client";

/**
 * The presence source for `hostId`, or `null` when that host's connection does not advertise
 * `presence`.
 *
 * The return type still names `Room` deliberately. Presence is a LiveKit concept and this stack does
 * not invent a neutral abstraction over participants — there is no second implementation of one, and
 * a wrapper with a single implementation would be a fiction. What changes is that reaching it now
 * requires asking, and asking can be refused. Node 4 gates the surfaces that ask.
 */
export function useHostPresence(hostId: string | null): Room | null {
  // TODO(host-directory): implement
  throw new Error(`useHostPresence(${hostId}) is not implemented yet`);
}
