/**
 * The transport-neutral connection model.
 *
 * `tddy-web` reaches three kinds of thing: the daemon that served this page (over `/rpc` or the
 * host application's IPC bridge — see `../daemonTransport`), some *other* daemon host, and a
 * session running on one of those hosts. Only the first was ever expressed without naming a wire.
 * The other two were spelled as a LiveKit `Room` plus a target participant identity, which is why
 * a host that is not reachable over LiveKit could not be reached at all.
 *
 * A {@link HostConnection} names the second of those without naming a wire. A
 * {@link ConnectionProvider} is how a wire offers one. What a connection can *do* beyond plain RPC
 * is a {@link ConnectionCapability}, because that is a property of how you are connected and not of
 * the machine you are connected to: the same host is media-capable over LiveKit and not over IPC.
 *
 * PRD: `docs/dev/1-WIP/2026-09-05-optional-livekit-connection-model-prd.md`.
 */

import type { Client, Transport } from "@connectrpc/connect";
import type { DescService } from "@bufbuild/protobuf";

/**
 * What a connection can do beyond unary and streaming RPC.
 *
 * - `rpc` — unary and streaming calls. Every connection has it; it is listed so a capability set is
 *   never empty and so a caller can ask the same question of every capability.
 * - `media` — audio/video/screen tracks. LiveKit only: a frame pipe cannot carry a track.
 * - `presence` — a live roster of who else is connected. LiveKit only, for the same reason.
 *
 * The two optional ones are what the media and presence surfaces are gated on. A connection that
 * lacks one is not degraded — those surfaces simply do not apply to it.
 */
export type ConnectionCapability = "rpc" | "media" | "presence";

/**
 * How a connection is doing.
 *
 * Deliberately the same four states `CommonRoomStatus` already used, so the selector chrome and the
 * connection overlays keep reading one vocabulary. `idle` is "nothing has been asked of this yet",
 * which is distinct from `error` — the distinction that kept the presence panel claiming it was
 * connecting for as long as the tab stayed open.
 */
export type ConnectionStatus = "idle" | "connecting" | "connected" | "error";

/**
 * A live connection to one daemon host.
 *
 * Obtained from {@link useHostConnection}; never constructed by a call site. A connection is valid
 * for as long as the provider that issued it says so — read {@link status} rather than assuming a
 * non-null connection is usable.
 */
export interface HostConnection {
  /** The host this reaches, as the host directory names it (a daemon instance id). */
  readonly hostId: string;

  /** The provider that issued this connection (`"livekit"`, later `"ipc"`). Diagnostics only. */
  readonly providerId: string;

  readonly status: ConnectionStatus;

  /** Why the connection is unusable, when {@link status} is `"error"`; `null` otherwise. */
  readonly error: string | null;

  /**
   * What this connection can do. Read it through `useHasCapability` rather than inspecting the set
   * directly, so there is exactly one place that answers the question.
   */
  readonly capabilities: ReadonlySet<ConnectionCapability>;

  /**
   * A client for `service` on this host, memoised per connection: the same instance for the same
   * service while this connection holds, so a consumer keying an effect on the client does not tear
   * its stream down on every render.
   */
  clientFor<S extends DescService>(service: S): Client<S>;

  /**
   * The raw transport, for a caller that builds its own client — `useHostFanOut` hands one to a
   * caller-supplied `clientFor`, and the RPC playground issues calls without a service binding.
   * Prefer {@link clientFor} when the service is known at the call site.
   */
  transport(): Transport;
}

/**
 * A wire that can reach hosts.
 *
 * Registered into the {@link ConnectionProviderRegistry}; `tddy-web` never imports a provider
 * directly, so a host build (the desktop app) can contribute one the browser bundle does not carry.
 */
export interface ConnectionProvider {
  /** Stable identifier, used for precedence and for diagnostics. */
  readonly id: string;

  /**
   * A connection to `hostId`, or `null` when this provider cannot reach that host.
   *
   * Returning `null` is the normal answer, not a failure: the IPC provider claims exactly one host,
   * and the LiveKit provider claims none until its room is joined. A provider that *can* reach the
   * host but is not connected yet returns a connection whose `status` says so — the two cases are
   * different, and collapsing them is what would make "no LiveKit" look like "no such host".
   */
  connectHost(hostId: string): HostConnection | null;
}
