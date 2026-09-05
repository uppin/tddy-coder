/**
 * LiveKit as a connection provider — the first wire to offer itself through the registry.
 *
 * This is the whole of what daemon-level RPC used to say inline at every call site: address the
 * host's RPC-server participant (`daemon-{instanceId}`, see `lib/participantRole`) over the shared
 * common-room connection, using the transport factory from `../transportProvider` so the auth gate
 * and the traffic meter it wraps around every LiveKit transport still apply. Nothing about that
 * behaviour changes here; what changes is that a call site no longer has to know any of it.
 *
 * Everything LiveKit-shaped in the connection model lives in this file, so a build that does not
 * join a common room simply never renders {@link LiveKitConnections} and reaches its hosts over
 * whichever provider it did register.
 */

import { useMemo, type ReactNode } from "react";
import { createClient, type Client, type Transport } from "@connectrpc/connect";
import type { DescService } from "@bufbuild/protobuf";
import { ConnectionState, type Room } from "livekit-client";
import { daemonRpcIdentity } from "../../lib/participantRole";
import {
  useLiveKitTransportFactory,
  type LiveKitTransportOptions,
} from "../transportProvider";
import { ConnectionProviders, useConnectionProviders } from "./registry";
import type {
  ConnectionCapability,
  ConnectionProvider,
  ConnectionStatus,
  HostConnection,
} from "./types";

/** The id this provider registers under. Precedence is stated against it, so it is a constant. */
export const LIVEKIT_PROVIDER_ID = "livekit";

/**
 * A LiveKit room carries tracks and a participant roster alongside its data channel, so a host
 * reached this way can do everything: RPC, media, presence. Shared by every connection — the set is
 * a property of the wire, and no caller may mutate it.
 */
const LIVEKIT_CAPABILITIES: ReadonlySet<ConnectionCapability> = new Set<ConnectionCapability>([
  "rpc",
  "media",
  "presence",
]);

/** The peer half of the transport seam — `RpcTransportProvider`'s `liveKitFactory`. */
type LiveKitTransportFactory = (
  room: Room,
  targetIdentity: string,
  options?: LiveKitTransportOptions,
) => Transport;

/**
 * One host, reached at its RPC-server identity over a common-room connection.
 *
 * The transport is built on first use and then kept: a host talked to by four screens is one
 * transport and one client per service, which is what makes {@link HostConnection.clientFor} stable
 * enough to key an effect on. It is also why a fan-out that re-reads the same peer twice records one
 * client build rather than two.
 */
class LiveKitHostConnection implements HostConnection {
  readonly providerId = LIVEKIT_PROVIDER_ID;
  readonly capabilities = LIVEKIT_CAPABILITIES;
  /**
   * Always `null`: a peer that is not on the room is reported through {@link status}, and the
   * room's own failure to connect belongs to the common room rather than to one host on it.
   */
  readonly error: string | null = null;

  /** The participant that serves this host's daemon-level RPC. */
  private readonly identity: string;
  private builtTransport: Transport | null = null;
  private readonly clients = new Map<DescService, Client<DescService>>();

  constructor(
    readonly hostId: string,
    private readonly room: Room,
    private readonly factory: LiveKitTransportFactory,
  ) {
    this.identity = daemonRpcIdentity(hostId);
  }

  /**
   * Read from the room at the moment it is asked, never captured: a host leaves the common room
   * without anything re-resolving this connection, and a caller refusing to send into a stream
   * nobody reads (`useAcpSessionOverClient`'s `peer`) has to see that at the moment of the send.
   *
   * `connecting` covers both "the room is not up yet" and "the room is up and this host is not on
   * it" — from a caller's side those are the same claim, that the host cannot be reached right now
   * and may be able to be in a moment. Neither is an `error`: an absent peer is an ordinary state of
   * a fleet whose machines come and go.
   */
  get status(): ConnectionStatus {
    if (this.room.state !== ConnectionState.Connected) return "connecting";
    return this.room.remoteParticipants.has(this.identity) ? "connected" : "connecting";
  }

  transport(): Transport {
    this.builtTransport ??= this.factory(this.room, this.identity);
    return this.builtTransport;
  }

  clientFor<S extends DescService>(service: S): Client<S> {
    const cached = this.clients.get(service);
    if (cached) return cached as Client<S>;
    const built = createClient(service, this.transport());
    this.clients.set(service, built as Client<DescService>);
    return built;
  }
}

/**
 * Reaches every host over one common-room LiveKit connection.
 *
 * It claims **no** host until it has a room: with the join still in flight — or never attempted,
 * which is the desktop app's default — there is no way to name a participant, and saying so is what
 * lets a screen render its ordinary "not connected yet" state instead of failing. Once there is a
 * room it claims every host asked of it, because a common room's roster is the host *directory*'s
 * business and not this provider's: a daemon that is not on the room yields a connection whose
 * `status` says `connecting`, which is a different claim from "no such host".
 *
 * A provider instance is bound to one room and one transport factory. A new room means a new
 * provider registered over the old one, which is what drops every client built against the wire
 * that went away.
 */
export class LiveKitConnectionProvider implements ConnectionProvider {
  readonly id = LIVEKIT_PROVIDER_ID;

  /** One connection per host, so `clientFor` is stable for as long as this provider is registered. */
  private readonly connections = new Map<string, HostConnection>();

  constructor(
    private readonly room: Room | null,
    private readonly factory: LiveKitTransportFactory,
  ) {}

  connectHost(hostId: string): HostConnection | null {
    if (!this.room || hostId === "") return null;
    const existing = this.connections.get(hostId);
    if (existing) return existing;
    const connection = new LiveKitHostConnection(hostId, this.room, this.factory);
    this.connections.set(hostId, connection);
    return connection;
  }
}

/**
 * Register the common room as a connection provider for the subtree, and make sure the subtree has
 * a registry to read it from.
 *
 * The registry is the one in scope when there is one — the app root's, which a host build may have
 * already registered its own wire into, ahead of this one — and a private one otherwise, so a screen
 * mounted on its own still reaches its hosts. Either way it is re-provided downwards, so what a call
 * site resolves through and what this component registered into are the same registry.
 *
 * Registration happens while this component renders, not in an effect, because the subtree's first
 * paint has to resolve its hosts: a screen that reads its fleet on mount would otherwise record
 * "no connection to daemon X" against every host before the wire that reaches them was offered, and
 * keep that answer. Registering the same provider instance twice is a no-op, so a re-run or a
 * discarded render leaves the registry exactly as it was.
 *
 * Nothing unregisters on unmount. A registration says a wire exists, and the registry it says it
 * into is either this component's own (which goes with it) or an ancestor's that outlives the whole
 * daemon-mode session; a remount re-registers and replaces the entry in place.
 */
export function LiveKitConnections({
  room,
  children,
}: {
  room: Room | null;
  children: ReactNode;
}) {
  const registry = useConnectionProviders();
  const factory = useLiveKitTransportFactory();
  const provider = useMemo(() => new LiveKitConnectionProvider(room, factory), [room, factory]);
  useMemo(() => registry.register(provider), [registry, provider]);
  return <ConnectionProviders registry={registry}>{children}</ConnectionProviders>;
}
