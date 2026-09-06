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

import { useRef, type ReactNode } from "react";
import { createClient, type Client, type Transport } from "@connectrpc/connect";
import type { DescService } from "@bufbuild/protobuf";
import { ConnectionState, Room } from "livekit-client";
import { daemonRpcIdentity } from "../../lib/participantRole";
import { TokenService } from "../../gen/token_pb";
import {
  useHttpClient,
  useLiveKitTransportFactory,
  type LiveKitTransportOptions,
} from "../transportProvider";
import { openHostServedSession } from "./hostServedSession";
import {
  openLiveKitSession,
  type RoomBackedHint,
  type TokenRefreshPolicy,
} from "./livekit/sessionConnection";
import { ConnectionProviders, useConnectionProviders } from "./registry";
import type { SessionAttachmentHint, SessionConnection } from "./session";
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
 * What opening a *session* connection needs on top of what reaching a host needs.
 *
 * A host is reached on the common room this provider is already bound to, so nothing beyond a
 * transport is required. A session lives in a room of its own: it has to be joined, which means a
 * browser token has to be minted and refreshed, and a `Room` object has to be constructed. Neither
 * is available inside `connectHost` — both come from hooks — so they are handed in at registration.
 *
 * `null` is a legitimate value: a provider constructed without these still reaches every host and
 * still opens a session whose hint names no room. Only a room-backed session needs them, and a
 * caller asking for one without them gets a refusal rather than a connection that joins nothing.
 */
export interface LiveKitSessionResources {
  /** Mints and refreshes the browser token a session room is joined with. */
  readonly tokens: Client<typeof TokenService>;

  /** Constructs the `Room` object to join — the injection seam `useCommonRoom` calls `roomFactory`. */
  readonly newRoom: () => Room;

  /** Overrides for a session connection's token-renewal schedule (see
   *  `DEFAULT_TOKEN_REFRESH_POLICY`). Production passes none; a spec that has to watch a renewal
   *  happen cannot wait the real hour. */
  readonly refreshPolicy?: Partial<TokenRefreshPolicy>;
}

/**
 * One host, reached at its RPC-server identity over a common-room connection.
 *
 * The transport is built on first use and then kept: a host talked to by four screens is one
 * transport and one client per service, which is what makes {@link HostConnection.clientFor} stable
 * enough to key an effect on. It is also why a fan-out that re-reads the same peer twice opens one
 * transport rather than two — note that this dedupes the *transport*, not the client: `useHostFanOut`
 * takes {@link transport} and builds its own client through the caller's `clientFor`, so it does get
 * a fresh client per effect run, over the one transport this connection holds.
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

  /**
   * `currentFactory` is read at transport-build time rather than captured, so a factory whose
   * *identity* churned — `RpcTransportProvider` rebuilds `lkFactory` on every one of its own
   * renders — does not have to invalidate a connection that is otherwise still good.
   */
  constructor(
    readonly hostId: string,
    private readonly room: Room,
    private readonly currentFactory: () => LiveKitTransportFactory,
    private readonly currentSessionResources: () => LiveKitSessionResources | null = () => null,
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
    this.builtTransport ??= this.currentFactory()(this.room, this.identity);
    return this.builtTransport;
  }

  clientFor<S extends DescService>(service: S): Client<S> {
    const cached = this.clients.get(service);
    if (cached) return cached as Client<S>;
    const built = createClient(service, this.transport());
    this.clients.set(service, built as Client<DescService>);
    return built;
  }

  /**
   * A connection to one session on this host.
   *
   * A hint naming a room is a session with a wire of its own: it is joined, and addressed at the
   * participant its own process serves on. A hint naming none is a session this host answers itself
   * — `cli_session_manager.rs` hosts `terminal.TerminalService` against a PTY handle — so it routes
   * over this connection and advertises `rpc` alone. That second case is the one the app currently
   * calls `connected-grpc` and treats as degraded; it is neither degraded nor a different kind of
   * thing, only a session whose RPC happens to arrive by the same road as its host's.
   *
   * Not memoised, unlike {@link clientFor}: two attachments of the same session are two attachments,
   * each with its own claim and its own `close()`, and handing the second one the first's connection
   * would make either close release both.
   */
  openSession(sessionId: string, hint: SessionAttachmentHint): SessionConnection {
    if (hint.sessionId !== sessionId) {
      throw new Error(
        `openSession(${sessionId}) was given a hint for session ${hint.sessionId}`,
      );
    }
    const { room } = hint;
    if (room === undefined) return openHostServedSession(this, hint);
    const resources = this.currentSessionResources();
    if (!resources) {
      throw new Error(
        `session ${sessionId} names LiveKit room ${room}, but this provider was registered ` +
          `without the token client and room factory needed to join one`,
      );
    }
    const roomBacked: RoomBackedHint = { ...hint, room };
    return openLiveKitSession(this.hostId, roomBacked, {
      tokens: resources.tokens,
      newRoom: resources.newRoom,
      refreshPolicy: resources.refreshPolicy,
      transportFor: (room, targetIdentity) => this.currentFactory()(room, targetIdentity),
    });
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
 * A provider instance is bound to one room, and only to one room: a new room means a new provider
 * registered over the old one, which is what drops every client built against the wire that went
 * away and what tells the registry to re-resolve every host. The transport *factory* is not part of
 * that identity — see {@link rebindFactory}.
 */
export class LiveKitConnectionProvider implements ConnectionProvider {
  readonly id = LIVEKIT_PROVIDER_ID;

  /** One connection per host, so `clientFor` is stable for as long as this provider is registered. */
  private readonly connections = new Map<string, HostConnection>();

  private factory: LiveKitTransportFactory;

  private sessionResources: LiveKitSessionResources | null;

  constructor(
    private readonly room: Room | null,
    factory: LiveKitTransportFactory,
    sessionResources: LiveKitSessionResources | null = null,
  ) {
    this.factory = factory;
    this.sessionResources = sessionResources;
  }

  /** Whether this provider is the one already bound to `room` — see {@link LiveKitConnections}. */
  isBoundTo(room: Room | null): boolean {
    return this.room === room;
  }

  /**
   * Swap the transport factory without disturbing anything else.
   *
   * `RpcTransportProvider` builds `lkFactory` fresh on every one of its renders, so the factory this
   * provider was constructed with goes out of date by identity long before it goes out of date by
   * behaviour. Replacing the provider over that would drop every transport and client built against
   * a wire that never went anywhere; swapping the field leaves the standing connections alone and
   * hands the new factory to the next transport that is built.
   */
  rebindFactory(factory: LiveKitTransportFactory): void {
    this.factory = factory;
  }

  /**
   * Swap what a session join is performed with, on the same terms as {@link rebindFactory}.
   *
   * `newRoom` is an inline arrow at the registration site, so this object's identity churns every
   * render while what it does never changes. Standing connections read through it at open time and
   * are left alone.
   */
  rebindSessionResources(sessionResources: LiveKitSessionResources | null): void {
    this.sessionResources = sessionResources;
  }

  connectHost(hostId: string): HostConnection | null {
    if (!this.room || hostId === "") return null;
    const existing = this.connections.get(hostId);
    if (existing) return existing;
    const connection = new LiveKitHostConnection(
      hostId,
      this.room,
      () => this.factory,
      () => this.sessionResources,
    );
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
 * keep that answer. (An effect-based version was tried and regressed exactly that —
 * `ModelsCatalogStateAcceptance` and `ModelsFanOutLifecycleAcceptance`.)
 *
 * Registering from render is only safe if it is idempotent *across instances*, which is why the
 * provider is held in a ref rather than derived with `useMemo`. A memo factory is allowed to run for
 * a render React then throws away — StrictMode double-invokes it, and a concurrent render may be
 * discarded — and each such run would have built a second provider object. `register` matches on id,
 * so the second object would replace the first, bump {@link ConnectionProviderRegistry.revision} and
 * invalidate every `useHostConnection`, every `useHostClient` and every cached connection in the app,
 * tearing down each effect keyed on a client. A ref survives a discarded render, so the registry
 * keeps being handed the object it already holds and `register` returns early.
 *
 * A *new room* is the one thing that must still produce a new instance: every transport built
 * against the room that went away is dead, and replacing the provider is how the registry learns
 * that and re-resolves each host — including the ordinary null-room-then-joined first paint. A
 * factory whose identity merely churned (`RpcTransportProvider` rebuilds `lkFactory` every render)
 * is not that, and is swapped into the provider standing.
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
  // The session-room token mint is an ordinary daemon RPC over this page's own transport, not
  // something reached through the common room — `useCommonRoom` mints its own token the same way.
  const tokens = useHttpClient(TokenService);
  const sessionResources: LiveKitSessionResources = { tokens, newRoom: () => new Room() };
  const providerRef = useRef<LiveKitConnectionProvider | null>(null);
  if (providerRef.current === null || !providerRef.current.isBoundTo(room)) {
    providerRef.current = new LiveKitConnectionProvider(room, factory, sessionResources);
  } else {
    providerRef.current.rebindFactory(factory);
    providerRef.current.rebindSessionResources(sessionResources);
  }
  // Called on every render rather than wrapped in a `useMemo`: a memo is a cache, not a scheduler,
  // and using one to perform a side effect makes the effect depend on whether React kept the render.
  // `register` is idempotent for the instance it already holds, so calling it unconditionally is
  // both cheaper to reason about and the only version that is actually correct.
  registry.register(providerRef.current);
  return <ConnectionProviders registry={registry}>{children}</ConnectionProviders>;
}
