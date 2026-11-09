/**
 * Host-connection fixtures for component specs.
 *
 * A screen takes the `HostConnection` the registry resolved and asks what it can do
 * (`capabilities`) and how it is doing (`status`). This builder states one of those in the terms a
 * spec cares about, with no wire behind it: calls land on an in-memory backend, and `status` can be
 * moved by the spec.
 *
 * `openSession` runs the production `openHostServedSession` over the host itself, so a session
 * opened on it reads its status through from the host — a session cannot be more reachable than the
 * daemon serving it. Capabilities come from the hint exactly as in production.
 *
 * Derived from `packages/tddy-web/cypress/support/rpc/hostConnections.ts`, which stays for the
 * tddy-web specs that still use it; this one differs in `openSession` and the movable status.
 */

import { createClient, type Client, type Transport } from "@connectrpc/connect";
import type { DescService } from "@bufbuild/protobuf";
import type {
  ConnectionCapability,
  ConnectionStatus,
  HostConnection,
} from "tddy-client-web/connections/types";
import { openHostServedSession } from "../../../src/rpc/connections/hostServedSession";

const A_PROVIDER = "in-memory";

export class HostConnectionBuilder {
  private capabilities = new Set<ConnectionCapability>(["rpc"]);
  private status: ConnectionStatus = "connected";
  private wire: Transport | null = null;

  constructor(private readonly hostId: string) {}

  /** The host's calls land on `transport` — normally an in-memory backend's. */
  servingOver(transport: Transport): HostConnectionBuilder {
    this.wire = transport;
    return this;
  }

  /** The host is not reachable yet; {@link admit} brings it up. */
  notReachableYet(): HostConnectionBuilder {
    this.status = "connecting";
    return this;
  }

  /** The daemon comes up — what the operator is waiting for behind the overlay. */
  admit(): void {
    this.status = "connected";
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
    const self = this;
    const host: HostConnection = {
      hostId: this.hostId,
      providerId: A_PROVIDER,
      get status() {
        return self.status;
      },
      error: null,
      capabilities: this.capabilities,
      clientFor: <S extends DescService>(service: S): Client<S> => {
        const cached = clients.get(service);
        if (cached) return cached as Client<S>;
        const built = createClient(service, transport());
        clients.set(service, built as Client<DescService>);
        return built;
      },
      transport,
      openSession: (_sessionId, hint) => openHostServedSession(host, hint),
    };
    return host;
  }
}

/** A host connection for `hostId`. See {@link HostConnectionBuilder} for the defaults. */
export function aHostConnection(hostId = "local"): HostConnectionBuilder {
  return new HostConnectionBuilder(hostId);
}
