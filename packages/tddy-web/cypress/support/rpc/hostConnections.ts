/**
 * Host-connection fixtures for component specs.
 *
 * A screen no longer takes a `Room` and a participant identity — it takes the `HostConnection` the
 * registry resolved, and asks what it can do (`capabilities`) and how it is doing (`status`). These
 * builders state one of those in the terms a spec cares about: a host reached over LiveKit, which
 * can carry tracks and a roster, or one reached over a wire that carries calls and nothing else.
 *
 * The sibling of `./sessionConnections.ts`, and deliberately the same shape.
 *
 * Model: `src/rpc/connections/types.ts`.
 */

import { createClient, type Client, type Transport } from "@connectrpc/connect";
import type { DescService } from "@bufbuild/protobuf";
import { ConnectionProviderRegistry } from "../../../src/rpc/connections/registry";
import type { SessionConnection } from "../../../src/rpc/connections/session";
import type {
  ConnectionCapability,
  ConnectionStatus,
  HostConnection,
} from "../../../src/rpc/connections/types";

const A_PROVIDER = "in-memory";

/**
 * Builds one host connection. Defaults to a connected host that carries plain RPC and nothing else
 * — what a frame pipe offers — so a spec states only what it is actually about.
 */
export class HostConnectionBuilder {
  private capabilities = new Set<ConnectionCapability>(["rpc"]);
  private status: ConnectionStatus = "connected";
  private failure: string | null = null;
  private wire: Transport | null = null;

  constructor(private readonly hostId: string) {}

  /** Reached over a common room — tracks and a participant roster come with that wire. */
  reachedOverLiveKit(): HostConnectionBuilder {
    this.capabilities = new Set<ConnectionCapability>(["rpc", "media", "presence"]);
    return this;
  }

  /** The host's calls land on `transport` — normally an in-memory backend's. */
  servingOver(transport: Transport): HostConnectionBuilder {
    this.wire = transport;
    return this;
  }

  /** The connection failed, and says why. */
  failedWith(error: string): HostConnectionBuilder {
    this.status = "error";
    this.failure = error;
    return this;
  }

  build(): HostConnection {
    const clients = new Map<DescService, Client<DescService>>();
    const transport = () => {
      if (!this.wire) {
        throw new Error(
          `host ${this.hostId} was built without \`servingOver(transport)\`, so it has nowhere ` +
            `to send this call — give the builder the backend's transport`,
        );
      }
      return this.wire;
    };

    return {
      hostId: this.hostId,
      providerId: A_PROVIDER,
      status: this.status,
      error: this.failure,
      capabilities: this.capabilities,
      clientFor: <S extends DescService>(service: S): Client<S> => {
        const cached = clients.get(service);
        if (cached) return cached as Client<S>;
        const built = createClient(service, transport());
        clients.set(service, built as Client<DescService>);
        return built;
      },
      transport,
      openSession: (): SessionConnection => {
        throw new Error(
          `host ${this.hostId} is a fixture for host-level reads; build the session connection ` +
            `directly with \`aSessionConnection(...)\` instead of attaching through it`,
        );
      },
    };
  }
}

/** A host connection for `hostId`. See {@link HostConnectionBuilder} for the defaults. */
export function aHostConnection(hostId = "local"): HostConnectionBuilder {
  return new HostConnectionBuilder(hostId);
}

/**
 * A registry whose only wire is the one that hands out `connections`, keyed by their `hostId`.
 *
 * The point of mounting a screen over this rather than over `withSelectedDaemon` is that the wire
 * is stated rather than implied: `withSelectedDaemon` hands the tree a `Room`, and a room means the
 * LiveKit provider claims every host with all three capabilities. A screen mounted here reaches its
 * host exactly as the builder said it does — over a frame pipe, or over a common room — which is
 * the only way to write the `{"rpc"}`-only half of a gating scenario.
 *
 * Pair it with `<SelectedDaemonProvider room={null} …>`: with no room, no `livekit-client` `Room` is
 * constructed anywhere in the tree and the LiveKit provider claims nothing, so this registry's
 * answer is the only one.
 */
export function aRegistryServing(...connections: HostConnection[]): ConnectionProviderRegistry {
  const registry = new ConnectionProviderRegistry();
  const byHostId = new Map(connections.map((connection) => [connection.hostId, connection]));
  registry.register({
    id: "fixture",
    connectHost: (hostId: string) => byHostId.get(hostId) ?? null,
  });
  return registry;
}
