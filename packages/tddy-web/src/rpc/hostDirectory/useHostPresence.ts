/**
 * Presence, as an explicitly requested capability rather than an ambient `Room`.
 *
 * `SelectedDaemonContextValue` used to publish `room: Room | null` to the whole subtree, so any
 * component could reach LiveKit without declaring that it needed presence — and the capability
 * gating in node 4 would have had no seam to gate on. This is that seam: a component that wants the
 * participant roster asks for it by name, and gets `null` on a connection that has none.
 *
 * Technical: `packages/tddy-web/docs/host-directory.md`.
 */

import type { Room } from "livekit-client";
import { useHostConnection } from "../connections/registry";
import { useHasCapability } from "../connections/useHasCapability";
import { usePresenceRoom } from "./presenceRoom";

/**
 * The presence source for `hostId`, or `null` when that host's connection does not advertise
 * `presence`.
 *
 * The return type still names `Room` deliberately. Presence is a LiveKit concept and this stack does
 * not invent a neutral abstraction over participants — there is no second implementation of one, and
 * a wrapper with a single implementation would be a fiction. What changes is that reaching it now
 * requires asking, and asking can be refused. Node 4 gates the surfaces that ask.
 *
 * Both halves of the answer are needed and neither is sufficient. The capability says whether a
 * roster applies to *this* host at all — an unreachable host, or one reached over a wire with no
 * roster, has none, and saying so is what a caller gates on. The room is where the roster actually
 * is, and it is not on `HostConnection`: `LiveKitHostConnection` keeps its own private, so that a
 * component holding a connection still cannot help itself to LiveKit.
 */
export function useHostPresence(hostId: string | null): Room | null {
  const connection = useHostConnection(hostId);
  const room = usePresenceRoom();
  // Through the predicate, not `capabilities.has(...)` inline: node 4 gates every presence surface
  // on `useHasCapability`, and this hook — the seam those surfaces reach presence through — reading
  // the set its own way would be the fourth reading of capability the single predicate exists to
  // prevent.
  const carriesPresence = useHasCapability(connection, "presence");
  return carriesPresence ? room : null;
}
