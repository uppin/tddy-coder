/**
 * The connection-provider registry: where wires (LiveKit, desktop IPC, an in-memory double) register
 * themselves and where a host is resolved to the first provider that can reach it.
 *
 * The package imports no provider. The app registers them; the hooks in `../ports/clients` resolve
 * a host through the registry the app supplied as `ClientsPort.connections`.
 */

import type { DescService } from "@bufbuild/protobuf";
import type { Client, Transport } from "@connectrpc/connect";
import type { SessionAttachmentHint, SessionConnection } from "./session";
import type {
  ConnectionCapability,
  ConnectionProvider,
  ConnectionStatus,
  HostConnection,
} from "./types";

/**
 * The capability a host's winning wire can **borrow** from a later wire that reaches the same host.
 *
 * Media alone, because media alone is not carried by the host connection: the media port joins the
 * room a stream's start reply names, with a token of its own, so a host whose own wire carries only
 * calls can still have its screen watched — provided some wire on this page reaches a media server
 * that host publishes into. RPC and presence are properties of the wire a host is *reached over*,
 * and borrowing them would route calls or rosters somewhere precedence said they must not go.
 */
const LENDABLE_CAPABILITIES: readonly ConnectionCapability[] = ["media"];

/**
 * The winning connection, with the lendable capabilities its lenders currently have.
 *
 * Everything but {@link capabilities} is the winner's: calls, sessions and status never move off
 * the wire precedence chose. The capability set is read at the moment it is asked — a lender's
 * status changes as its room connects and its peers come and go, without anything re-resolving
 * this host, exactly as a LiveKit connection's own `status` does.
 */
class BorrowingMediaConnection implements HostConnection {
  constructor(
    private readonly winner: HostConnection,
    private readonly lenders: readonly HostConnection[],
  ) {}

  get hostId(): string {
    return this.winner.hostId;
  }

  get providerId(): string {
    return this.winner.providerId;
  }

  get status(): ConnectionStatus {
    return this.winner.status;
  }

  get error(): string | null {
    return this.winner.error;
  }

  get capabilities(): ReadonlySet<ConnectionCapability> {
    const lent = LENDABLE_CAPABILITIES.filter((capability) =>
      this.lenders.some(
        (lender) => lender.status === "connected" && lender.capabilities.has(capability),
      ),
    );
    if (lent.length === 0) return this.winner.capabilities;
    return new Set([...this.winner.capabilities, ...lent]);
  }

  clientFor<S extends DescService>(service: S): Client<S> {
    return this.winner.clientFor(service);
  }

  transport(): Transport {
    return this.winner.transport();
  }

  openSession(sessionId: string, hint: SessionAttachmentHint): SessionConnection {
    return this.winner.openSession(sessionId, hint);
  }
}

/**
 * The registered providers, in precedence order.
 *
 * Precedence is registration order, first match wins, and it is load-bearing exactly once: the
 * desktop build registers its IPC provider ahead of the LiveKit one, so its own host is reached
 * in-process even when a common room is configured and could also reach it. Expressing that as
 * order rather than as a preference setting is why there is no `preferIpc` flag anywhere.
 *
 * The registry is mutable and long-lived — a wire registers itself when it comes up, and comes up
 * again on a different connection later. So it is also an observable store (`subscribe`/{@link
 * revision}, the shape `sessionNotificationRegistry` already uses): a call site that has no other
 * reason to re-render still learns that the host it could not reach a moment ago is now reachable.
 */
export class ConnectionProviderRegistry {
  private readonly providers: ConnectionProvider[] = [];
  private readonly listeners = new Set<() => void>();
  private currentRevision = 0;
  private notifyScheduled = false;
  private readonly borrowed = new WeakMap<
    HostConnection,
    { lenders: readonly HostConnection[]; connection: HostConnection }
  >();

  /** Append `provider`. A provider already registered under the same id replaces the earlier one. */
  register(provider: ConnectionProvider): void {
    const existing = this.providers.findIndex((p) => p.id === provider.id);
    if (existing >= 0) {
      // Re-registering the same instance is what a wire does on every render of the component that
      // offers it; treating that as a change would notify every consumer into rebuilding clients
      // that did not move.
      if (this.providers[existing] === provider) return;
      // Replaced where it stood, not appended: precedence is a deployment's decision, and a wire
      // that re-registers on reconnect must not overtake — or fall behind — the wires it was
      // ordered against.
      this.providers[existing] = provider;
    } else {
      this.providers.push(provider);
    }
    this.currentRevision += 1;
    this.scheduleNotify();
  }

  /**
   * The first registered provider that can reach `hostId`, or `null` when none can.
   *
   * Calls, sessions and status are that provider's alone. Its connection is handed back as it is
   * unless a provider behind it also reaches the host with media the winner lacks; then the winner
   * is returned borrowing that media (see {@link LENDABLE_CAPABILITIES}) for as long as the lender
   * is connected to the host.
   *
   * `null` is a normal answer — see `ConnectionProvider.connectHost`.
   */
  connectHost(hostId: string): HostConnection | null {
    const reaching = this.providers.flatMap((provider) => provider.connectHost(hostId) ?? []);
    const [winner, ...behind] = reaching;
    if (!winner) return null;
    const lenders = behind.filter((connection) =>
      LENDABLE_CAPABILITIES.some(
        (capability) =>
          !winner.capabilities.has(capability) && connection.capabilities.has(capability),
      ),
    );
    if (lenders.length === 0) return winner;
    return this.borrowing(winner, lenders);
  }

  /**
   * One borrowing connection per winner and set of lenders, so a host resolved twice is the same
   * object while its routing holds — `useHostConnection` memoises on it, and a consumer keys effects
   * on the clients behind it.
   */
  private borrowing(winner: HostConnection, lenders: readonly HostConnection[]): HostConnection {
    const held = this.borrowed.get(winner);
    if (held && sameMembers(held.lenders, lenders)) return held.connection;
    const connection = new BorrowingMediaConnection(winner, lenders);
    this.borrowed.set(winner, { lenders, connection });
    return connection;
  }

  /** The ids of the registered providers, in precedence order. Diagnostics and tests. */
  providerIds(): readonly string[] {
    return this.providers.map((provider) => provider.id);
  }

  /**
   * Tell the observers, once, after the current task.
   *
   * Deferred because a wire registers itself while the component that owns it renders — that is the
   * only moment early enough for the subtree's first paint to resolve its hosts — and a render may
   * not update other components. By the time the notification lands they have usually rendered with
   * the new routing already, read the same {@link revision} they were about to read anyway, and
   * `useSyncExternalStore` drops the update. What it does catch is the case that motivates the
   * subscription at all: a subtree that would otherwise not re-render, because the element it was
   * handed did not change.
   */
  private scheduleNotify(): void {
    if (this.notifyScheduled || this.listeners.size === 0) return;
    this.notifyScheduled = true;
    queueMicrotask(() => {
      this.notifyScheduled = false;
      for (const listener of this.listeners) listener();
    });
  }

  /** Observe registrations. Returns the unsubscribe. */
  subscribe(listener: () => void): () => void {
    this.listeners.add(listener);
    return () => {
      this.listeners.delete(listener);
    };
  }

  /**
   * How many times the provider list has changed — the `useSyncExternalStore` snapshot.
   *
   * A counter rather than the provider array itself, because the array is mutated in place: a
   * snapshot has to be a value that compares equal until something actually changed.
   */
  revision(): number {
    return this.currentRevision;
  }
}

function sameMembers(a: readonly HostConnection[], b: readonly HostConnection[]): boolean {
  return a.length === b.length && a.every((connection, index) => connection === b[index]);
}
