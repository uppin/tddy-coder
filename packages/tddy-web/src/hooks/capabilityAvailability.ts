/**
 * When a LiveKit-carried capability is on offer, and when it simply is not.
 *
 * Two facts decide it and they are not interchangeable. The **capability** says whether the thing
 * applies to this connection at all: a host reached over a wire that carries no presence has no
 * participant roster, one that carries no tracks has no VNC picture, and neither ever will while
 * the host is reached that way. The common room's **status** says how a wire that does carry them
 * is getting on: still joining, joined, or failed with a reason.
 *
 * The order below is the whole point. A join in flight has not yet produced a connection with any
 * capability on it — `LiveKitConnections` is bound to a `null` room until `Room.connect()` resolves,
 * and a join that failed produces no connection at all. A surface that asked the capability first
 * would therefore announce "not available on this connection" for the second or two every LiveKit
 * page spends connecting, and then contradict itself. Status first, capability second: "still
 * joining" and "failed, here is why" are answers about a capability that exists, and only a
 * connection with neither of those going on can be said not to have it.
 *
 * Both worked examples are real and both are why this exists:
 *
 * - **Presence.** Reading the capability alone would replace the reason a join failed with a verdict
 *   about the wire — the 2026-08-13 `udoo` incident, where ICE never established and every symptom
 *   the operator could see was indistinguishable from "still connecting". That reason reaching the
 *   participant panel is pinned by `CommonRoomConnectionVisibilityAcceptance`.
 * - **Media.** The session inspector's tab strip would render seven tabs on load and nine a second
 *   later, reflowing under the operator's cursor, because the VNC and Screen Sharing tabs were
 *   waiting on a connection the common-room join had not produced yet.
 *
 * A host that never joins a room — the desktop build over IPC — reports `idle`, never `connecting`,
 * so treating `connecting` as "keep it" cannot make a surface appear and then vanish there.
 *
 * One function rather than the same three lines in six components, so they cannot drift into
 * disagreeing about what the operator is being told on the same page — in particular a navigation
 * entry and the screen it points at. It lives beside `participantCameraVideo.ts` for the same
 * reason that one does: a pure rule about a LiveKit-shaped surface, testable without React.
 *
 * **Call it through `useCapabilityAvailability` unless you are handed the status.** Centralising the
 * rule left every surface still assembling its two arguments by hand — resolve the connection, read
 * the common room out of the host directory, apply `useHasCapability`, default a missing source to
 * `idle` — which is the same drift one level up. The hook next door does all of that once. Only
 * `ParticipantList` calls this directly, because it is presentational and is *told* which room's
 * status it is reporting on.
 *
 * PRD: `docs/dev/1-WIP/2026-09-05-optional-livekit-capability-gating-prd.md` (AC 2, AC 3, AC 7).
 */

import type { CommonRoomStatus } from "./useCommonRoom";

/**
 * What a capability-gated surface should say.
 *
 * - `error` — the join failed; quote the reason, and keep the surface that would report it.
 * - `connecting` — the join is in flight; say so and claim nothing else.
 * - `unavailable` — this connection does not carry the capability; name that as the reason.
 * - `available` — it is there; render as normal.
 */
export type CapabilityAvailability = "error" | "connecting" | "unavailable" | "available";

/**
 * Resolve the two facts into the one thing a surface renders.
 *
 * `hasCapability` is always {@link useHasCapability}'s answer — the single predicate — never a
 * re-derivation from a transport, a status string or the presence of a `Room`.
 */
export function capabilityAvailability(
  roomStatus: CommonRoomStatus,
  hasCapability: boolean,
): CapabilityAvailability {
  if (roomStatus === "error") return "error";
  if (roomStatus === "connecting") return "connecting";
  return hasCapability ? "available" : "unavailable";
}
