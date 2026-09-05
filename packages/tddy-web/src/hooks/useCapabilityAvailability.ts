/**
 * The hook every capability-gated surface asks, so that none of them assembles the answer itself.
 *
 * {@link capabilityAvailability} was centralised so the surfaces "cannot drift" — but the drift
 * surface had merely moved up a level. Six components still re-assembled the *same three lines*
 * around it: resolve the connection, read the common room out of the host directory, apply
 * `useHasCapability`, default a missing source to `idle`. Six copies of an argument list are six
 * chances to pass the wrong capability, read a different source, or default the missing status the
 * other way — which is exactly the disagreement between a navigation entry and the screen it points
 * at that the rule exists to prevent. There is now one copy, here, and a call site states only the
 * two things that are genuinely its own: which connection, and which capability.
 *
 * The `?? "idle"` lives here and nowhere else. An absent LiveKit source is "no common room was ever
 * asked for" — the desktop build, and every component test that mounts no `HostDirectorySources` —
 * and reading it as anything but `idle` would either invent a join in flight or invent a failure.
 *
 * The capability half is always {@link useHasCapability}: this hook composes the one predicate, it
 * does not re-derive one from a transport, a status string or the presence of a `Room`.
 *
 * Kept beside {@link capabilityAvailability} rather than inside it so that the rule itself stays a
 * pure function with no React and no host-directory import — that is what lets
 * `capabilityAvailability.test.ts` state the whole truth table without rendering anything.
 *
 * Changeset: `docs/dev/1-WIP/2026-09-05-optional-livekit-capability-gating.md`.
 * PRD: `docs/dev/1-WIP/2026-09-05-optional-livekit-capability-gating-prd.md` (AC 2, AC 3, AC 7).
 */

import { LIVEKIT_SOURCE_ID } from "../rpc/hostDirectory/liveKitSource";
import { useHostDirectorySource } from "../rpc/hostDirectory/useHostDirectory";
import { useHasCapability, type CapabilityBearing } from "../rpc/connections/useHasCapability";
import { capabilityAvailability, type CapabilityAvailability } from "./capabilityAvailability";
import type { ConnectionCapability } from "../rpc/connections/types";

/**
 * What `connection` can offer of `capability` right now, read in the one fixed order.
 *
 * Takes a connection rather than a host id because one caller — `SessionInspectorDrawer` — is
 * *handed* the host connection as a prop and has no id to resolve from; every other caller already
 * holds the connection for other reasons. A hook that took an id would leave that one site
 * assembling the answer by hand, which is the thing this exists to stop.
 *
 * **The common room is one source for the whole page, and the capability is one host's.** So while
 * the join is in flight — or permanently in `error` — this answers "not unavailable" for *every*
 * host, including one reached over a wire that provably cannot carry the capability. That is
 * harmless today (see the changeset's ordering-rule section) and node 7 of the `optional-livekit`
 * stack, which introduces a fleet where one host is reached over IPC and another over LiveKit, is
 * where it stops being harmless.
 */
export function useCapabilityAvailability(
  connection: CapabilityBearing | null | undefined,
  capability: ConnectionCapability,
): CapabilityAvailability {
  const commonRoom = useHostDirectorySource(LIVEKIT_SOURCE_ID);
  const carriesIt = useHasCapability(connection, capability);
  return capabilityAvailability(commonRoom?.status ?? "idle", carriesIt);
}
