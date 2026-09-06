/**
 * The one predicate every media and presence surface is gated on.
 *
 * Nodes 1 and 3 put `capabilities` on host and session connections. This is what reads it. There is
 * deliberately exactly one such function: three nodes have now added capability information, and a
 * fourth place that re-derived "can I show video here" from the presence of a `Room` is precisely
 * the drift that would undo the stack.
 *
 * Technical: `packages/tddy-web/docs/capability-gating.md`.
 */

import type { ConnectionCapability } from "./types";

/** The part of a connection this predicate reads — host or session, they answer the same way. */
export interface CapabilityBearing {
  readonly capabilities: ReadonlySet<ConnectionCapability>;
}

/**
 * Whether `connection` can do `capability`.
 *
 * A `null` connection answers `false`: nothing is selected, or nothing can reach it, and either way
 * there is no surface to show. That collapse is intentional — a caller that needs to tell "no host"
 * from "host without video" is asking a routing question, not a capability one, and should read the
 * connection's `status`.
 */
export function useHasCapability(
  connection: CapabilityBearing | null | undefined,
  capability: ConnectionCapability,
): boolean {
  return hasCapability(connection, capability);
}

/**
 * The non-hook form, for pure code and for tests.
 *
 * `useHasCapability` is a hook only for symmetry with the rest of `src/rpc`; the answer needs no
 * React state. Anything outside a component calls this.
 */
export function hasCapability(
  connection: CapabilityBearing | null | undefined,
  capability: ConnectionCapability,
): boolean {
  return connection?.capabilities.has(capability) ?? false;
}
