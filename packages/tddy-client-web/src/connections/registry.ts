/**
 * The connection-provider registry: where wires (LiveKit, desktop IPC, an in-memory double) register
 * themselves and where a host is resolved to the first provider that can reach it.
 *
 * The package imports no provider. The app registers them; the hooks in `../ports/clients` resolve
 * a host through the registry the app supplied as `ClientsPort.connections`.
 */

import type { ConnectionProvider, HostConnection } from "./types";

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
   * `null` is a normal answer — see `ConnectionProvider.connectHost`.
   */
  connectHost(hostId: string): HostConnection | null {
    for (const provider of this.providers) {
      const connection = provider.connectHost(hostId);
      if (connection) return connection;
    }
    return null;
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
