/**
 * Where the presence source lives now that it is not on the daemon-selection context.
 *
 * `useHostPresence` has to answer two questions that no single owner can: *may* this host's
 * connection offer a participant roster, and *what* is the roster. The first belongs to
 * `HostConnection.capabilities` (`../connections/types`), which node 1 owns and this node consumes
 * as it stands — `LiveKitHostConnection` keeps its `Room` private on purpose, so a component
 * holding a connection cannot reach LiveKit without asking.
 *
 * The second is this context. The wiring that already owns the common room populates it; nothing
 * else reads it directly.
 */

import { createContext, useContext, type ReactNode } from "react";
import type { Room } from "livekit-client";

const PresenceRoomContext = createContext<Room | null>(null);

export interface HostPresenceRoomProps {
  /** The joined common room, or `null` while there is none — including when there never will be. */
  room: Room | null;
  children: ReactNode;
}

/**
 * Offer the common room to the subtree as the presence source for hosts reached over it. Mounted by
 * whoever owns the join (`SelectedDaemonProvider`).
 */
export function HostPresenceRoom({ room, children }: HostPresenceRoomProps) {
  return <PresenceRoomContext.Provider value={room}>{children}</PresenceRoomContext.Provider>;
}

/**
 * The room in scope, ungated.
 *
 * **Only `useHostPresence` should call this.** Reading it directly is the ambient `room` on the
 * shared context all over again: it hands out LiveKit without the caller declaring that it wants
 * presence, which is exactly the seam node 4 gates on. It is exported rather than module-private
 * only because the gate lives in a sibling file; that is a limit of the file split, not a licence.
 */
export function usePresenceRoom(): Room | null {
  return useContext(PresenceRoomContext);
}
