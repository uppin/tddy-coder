/**
 * Whether a host-scoped screen may render, decided from what the connection layer offers.
 *
 * A screen scoped to the selected host (its projects, sessions, accounts, worktrees, VMs) has
 * nothing it can do without an RPC client that reaches that host. Rendering it anyway is how a
 * LiveKit server going down became a Create button that did nothing: the common room's join failed,
 * so no connection reached the host and every call site returned early. Such a screen is blocked
 * instead, and says why.
 *
 * Blocked only when there is **no viable client**: no host selected, no connection for it, or a
 * LiveKit connection whose common room has failed. A connection that exists while the room is still
 * settling stays open — `LiveKitHostConnection` reports `connecting` for that, which is not a failure
 * (see `ConnectionStatus`), and the single calls made on it report their own failures inline.
 */

import { LIVEKIT_PROVIDER_ID } from "../../rpc/connections/liveKit";
import type { ConnectionStatus } from "../../rpc/connections/types";

export interface HostGateInput {
  selectedInstanceId: string | null;
  /** The selected host's display name (`useHostLabel`); its instance id stands in when it has none. */
  hostLabel: string | null;
  /** The selected host's connection, or `null` when no provider reaches it. */
  connection: { providerId: string } | null;
  /** The common room's directory source, when this page has one. */
  liveKit: { status: ConnectionStatus; error: string | null } | undefined;
}

export type HostGate =
  | { kind: "open" }
  | {
      kind: "blocked";
      /** `connecting` while a join is under way and may yet succeed; `error` otherwise. */
      tone: "connecting" | "error";
      title: string;
      reason: string | null;
    };

export function hostGateOf({ selectedInstanceId, hostLabel, connection, liveKit }: HostGateInput): HostGate {
  if (!selectedInstanceId) {
    return {
      kind: "blocked",
      tone: "error",
      title: "No host selected",
      reason: "Choose a host from the selector above.",
    };
  }
  const host = hostLabel || selectedInstanceId;
  const roomFailure = liveKit?.status === "error" ? liveKit.error || "the common room failed" : null;

  if (connection) {
    if (connection.providerId !== LIVEKIT_PROVIDER_ID || !roomFailure) return { kind: "open" };
    return { kind: "blocked", tone: "error", title: `Not connected to ${host}`, reason: `LiveKit: ${roomFailure}` };
  }

  if (roomFailure) {
    return { kind: "blocked", tone: "error", title: `Not connected to ${host}`, reason: `LiveKit: ${roomFailure}` };
  }
  if (liveKit?.status === "connecting") {
    return { kind: "blocked", tone: "connecting", title: `Connecting to ${host}…`, reason: null };
  }
  if (liveKit?.status === "connected") {
    return {
      kind: "blocked",
      tone: "error",
      title: `Not connected to ${host}`,
      reason: `${host} is not in the common room — it may be offline.`,
    };
  }
  return {
    kind: "blocked",
    tone: "error",
    title: `Not connected to ${host}`,
    reason: "No connection reaches this host: LiveKit is not configured for this page.",
  };
}
