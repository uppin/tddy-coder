import { useRef } from "react";
import type { Client } from "@connectrpc/connect";
import type { ConnectionService } from "../../gen/connection_pb";

type ConnectionClient = Client<typeof ConnectionService>;

/**
 * Per-host cache of session-scoped `ConnectionService` clients, keyed by routing target.
 *
 * Hosts build a session's client inline while rendering (`buildSessionClient`), so without a cache
 * every render mints a fresh `Client` for an unchanged target. Consumers that key an effect on the
 * client — the Agent Activity feeds in `useAcpReplay`, for one — would then tear down and re-open
 * their stream on every host render, cancelling an in-flight snapshot pull. Resolving each build
 * through {@link SessionClientCache.clientFor} makes a client's identity as stable as its routing:
 * the same instance while the target and its transport hold, a fresh one as soon as either genuinely
 * changes (a reconnect hands back a new `Room`; another session targets another participant), so a
 * real routing upgrade is still picked up.
 */
export class SessionClientCache {
  private readonly entries = new Map<string, { transportKey: object; client: ConnectionClient }>();

  /**
   * The client routing to `targetIdentity` over `transportKey`, created via `create` on first use.
   *
   * `transportKey` is the object whose identity defines the transport — the session's LiveKit `Room`
   * in production. It is compared by reference: an unchanged reference reuses the cached client, a
   * new one replaces it.
   */
  clientFor(
    targetIdentity: string,
    transportKey: object,
    create: () => ConnectionClient,
  ): ConnectionClient {
    const cached = this.entries.get(targetIdentity);
    if (cached && cached.transportKey === transportKey) return cached.client;
    const client = create();
    this.entries.set(targetIdentity, { transportKey, client });
    return client;
  }
}

/** A {@link SessionClientCache} that lives as long as the host component instance. */
export function useSessionClientCache(): SessionClientCache {
  const cacheRef = useRef<SessionClientCache | null>(null);
  cacheRef.current ??= new SessionClientCache();
  return cacheRef.current;
}
