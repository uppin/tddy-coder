/**
 * When a participant roster is on offer, and when it simply is not.
 *
 * Two facts decide it and they are not interchangeable. The **capability** says whether a roster
 * applies to this connection at all: a host reached over a wire that carries no LiveKit presence has
 * none, and never will while it is reached that way. The common room's **status** says how a roster
 * that does apply is getting on: still joining, joined, or failed with a reason.
 *
 * The order below is the whole point. A join in flight has not yet produced a connection with
 * `presence` on it — `LiveKitConnections` is bound to a `null` room until `Room.connect()` resolves —
 * so a surface that asked the capability first would announce "not available on this connection"
 * for the second or two every LiveKit page spends connecting, and then contradict itself. Status
 * first, capability second: "still joining" and "failed, here is why" are answers about a roster
 * that exists, and only a connection with neither of those going on can be said to have no roster.
 *
 * One function rather than the same three lines in `ParticipantList`, `LiveKitAppPage` and
 * `SessionsDrawerScreen`, so those three cannot drift into disagreeing about what the operator is
 * being told on the same page. It lives beside `participantCameraVideo.ts` for the same reason that
 * one does: a pure rule about a LiveKit-shaped surface, testable without React.
 *
 * PRD: `docs/dev/1-WIP/2026-09-05-optional-livekit-capability-gating-prd.md` (AC 3, AC 7).
 */

import type { CommonRoomStatus } from "./useCommonRoom";

/**
 * What a presence-derived surface should say.
 *
 * - `error` — the join failed; quote the reason.
 * - `connecting` — the join is in flight; say so and claim nothing else.
 * - `unavailable` — this connection carries no presence; name that as the reason.
 * - `available` — there is a roster; render it.
 */
export type PresenceAvailability = "error" | "connecting" | "unavailable" | "available";

/**
 * Resolve the two facts into the one thing a surface renders.
 *
 * `hasPresence` is always {@link useHasCapability}'s answer — the single predicate — never a
 * re-derivation from a transport, a status string or the presence of a `Room`.
 */
export function presenceAvailability(
  roomStatus: CommonRoomStatus,
  hasPresence: boolean,
): PresenceAvailability {
  if (roomStatus === "error") return "error";
  if (roomStatus === "connecting") return "connecting";
  return hasPresence ? "available" : "unavailable";
}
