/**
 * Acceptance spec: daemon-level RPC resolves through the connection-provider registry, with no
 * LiveKit object anywhere in the tree.
 *
 * This is the node's headline claim. Today `useDaemonClientFor` is
 * `useLiveKitClient(service, room, daemonRpcIdentity(instanceId))` — a call site cannot ask for a
 * host without holding a `livekit-client` `Room`, which is why `tddy-desktop`, not joining a common
 * room by default, can reach no host at all. After this node a call site asks the registry, and a
 * host build supplies the wire.
 *
 * The "no LiveKit" guarantee here is structural rather than a spy: the only provider registered is
 * an in-memory one, so there is nothing that *could* construct a `Room`. A spy would prove less and
 * break the moment the LiveKit provider moved file.
 *
 * Changeset: `docs/dev/1-WIP/2026-09-05-optional-livekit-connection-model.md`
 * Stack: `optional-livekit` node 1 of 7.
 */

import React from "react";
import { createClient, type Client } from "@connectrpc/connect";
import type { DescService } from "@bufbuild/protobuf";
import { create } from "@bufbuild/protobuf";
import { anInMemoryRpcBackend } from "tddy-connectrpc-testkit";
import {
  ConnectionService,
  ListSessionsResponseSchema,
  SessionEntrySchema,
} from "../../src/gen/connection_pb";
import {
  ConnectionProviders,
  ConnectionProviderRegistry,
  useHostClient,
  useHostConnection,
} from "../../src/rpc/connections/registry";
import type { ConnectionProvider, HostConnection } from "../../src/rpc/connections/types";
import { byTestId } from "../support/testIds";

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

const THIS_HOST = "instance-this-host";
const A_PEER = "instance-a-peer";

/** A provider serving `hosts` over `backend`, with no wire of its own. */
function anInMemoryProviderNamed(
  id: string,
  hosts: string[],
  backend: ReturnType<typeof anInMemoryRpcBackend>,
): ConnectionProvider {
  const transport = backend.transport();
  const connections = new Map<string, HostConnection>();
  return {
    id,
    connectHost: (hostId) => {
      if (!hosts.includes(hostId)) return null;
      // One connection per host, so a test can assert client identity is as stable as routing.
      const existing = connections.get(hostId);
      if (existing) return existing;
      // One client per service per connection, so client identity is as stable as routing —
      // the guarantee `SessionClientCache` gives today, which a consumer keying an effect on
      // the client depends on.
      const clients = new Map<DescService, Client<DescService>>();
      const clientFor = <S extends DescService>(service: S): Client<S> => {
        const cached = clients.get(service);
        if (cached) return cached as Client<S>;
        const built = createClient(service, transport);
        clients.set(service, built as Client<DescService>);
        return built;
      };
      const connection: HostConnection = {
        hostId,
        providerId: id,
        status: "connected",
        error: null,
        capabilities: new Set(["rpc"]),
        clientFor,
        transport: () => transport,
      };
      connections.set(hostId, connection);
      return connection;
    },
  };
}

/** A registry holding one in-memory provider for `hosts`. No LiveKit provider is ever registered. */
function aRegistryServing(
  hosts: string[],
  backend: ReturnType<typeof anInMemoryRpcBackend>,
): ConnectionProviderRegistry {
  const registry = new ConnectionProviderRegistry();
  registry.register(anInMemoryProviderNamed("in-memory", hosts, backend));
  return registry;
}

// ---------------------------------------------------------------------------
// Probes
// ---------------------------------------------------------------------------

/** Lists a host's sessions through whatever wire the registry resolved. */
function SessionCountProbe({ hostId }: { hostId: string | null }) {
  const client = useHostClient(ConnectionService, hostId);
  const [label, setLabel] = React.useState("no client");

  React.useEffect(() => {
    if (!client) {
      setLabel("no client");
      return;
    }
    let cancelled = false;
    void client
      .listSessions({})
      .then((res) => {
        if (!cancelled) setLabel(`sessions: ${res.sessions.length}`);
      })
      .catch((err: unknown) => {
        if (!cancelled) setLabel(`error: ${String(err)}`);
      });
    return () => {
      cancelled = true;
    };
  }, [client]);

  return <div data-testid="session-count">{label}</div>;
}

/** Renders how many distinct client instances this component has been handed across its renders. */
function ClientIdentityProbe({ hostId }: { hostId: string | null }) {
  const client = useHostClient(ConnectionService, hostId);
  const seen = React.useRef(new Set<unknown>());
  const [, forceRender] = React.useState(0);
  if (client) seen.current.add(client);
  return (
    <div>
      <div data-testid="distinct-clients">{seen.current.size}</div>
      <button data-testid="re-render" onClick={() => forceRender((n) => n + 1)}>
        render again
      </button>
    </div>
  );
}

/** Names the provider that answered for `hostId`, or says nothing could reach it. */
function ResolvedProviderProbe({ hostId }: { hostId: string | null }) {
  const connection = useHostConnection(hostId);
  return <div data-testid="resolved-provider">{connection?.providerId ?? "unreachable"}</div>;
}

// ---------------------------------------------------------------------------
// Specs
// ---------------------------------------------------------------------------

describe("daemon-level RPC over the connection-provider registry", () => {
  it("reaches a host's ConnectionService with no LiveKit provider registered at all", () => {
    // Given a registry whose only provider is an in-memory one — there is nothing in this tree
    // that could construct a `livekit-client` Room, which is the desktop app's situation when no
    // common room is configured
    const backend = anInMemoryRpcBackend().onUnary(ConnectionService.method.listSessions, () =>
      create(ListSessionsResponseSchema, {
        sessions: [
          create(SessionEntrySchema, { sessionId: "a-session" }),
          create(SessionEntrySchema, { sessionId: "another-session" }),
        ],
      }),
    );

    // When a component asks for that host's daemon-level client and calls it
    cy.mount(
      <ConnectionProviders registry={aRegistryServing([THIS_HOST], backend)}>
        <SessionCountProbe hostId={THIS_HOST} />
      </ConnectionProviders>,
    );

    // Then the call is served — daemon-level RPC no longer requires a room and a participant
    byTestId("session-count").should("have.text", "sessions: 2");
  });

  it("hands back the same client instance across renders while the host is unchanged", () => {
    // Given a resolvable host
    const backend = anInMemoryRpcBackend();

    cy.mount(
      <ConnectionProviders registry={aRegistryServing([THIS_HOST], backend)}>
        <ClientIdentityProbe hostId={THIS_HOST} />
      </ConnectionProviders>,
    );

    // When the component re-renders several times without its host changing
    byTestId("re-render").click().click().click();

    // Then it was handed one client, not four. Consumers key effects on the client — the Agent
    // Activity feed in `useAcpReplay` is one — and a fresh instance per render tears their stream
    // down and cancels the snapshot pull in flight.
    byTestId("distinct-clients").should("have.text", "1");
  });

  it("returns no client for a host no provider claims, rather than failing", () => {
    // Given a registry that serves one host
    const backend = anInMemoryRpcBackend();

    // When a component asks for a different host — one that walked out of the common room, or a
    // peer a LiveKit-less desktop app cannot see
    cy.mount(
      <ConnectionProviders registry={aRegistryServing([THIS_HOST], backend)}>
        <SessionCountProbe hostId={A_PEER} />
      </ConnectionProviders>,
    );

    // Then the call site sees no client and renders its ordinary empty state. This is the
    // guard every consumer already has, and it is what lets LiveKit be absent.
    byTestId("session-count").should("have.text", "no client");
  });

  it("returns no client before a host is selected", () => {
    // Given a resolvable registry but no selection yet — the first paint of every daemon-mode screen
    const backend = anInMemoryRpcBackend();

    cy.mount(
      <ConnectionProviders registry={aRegistryServing([THIS_HOST], backend)}>
        <SessionCountProbe hostId={null} />
      </ConnectionProviders>,
    );

    // Then a null host is the same ordinary "nothing yet" as an unreachable one
    byTestId("session-count").should("have.text", "no client");
  });

  it("names the provider that answered, so a diagnostic can say which wire was used", () => {
    // Given a host served by the in-memory provider
    const backend = anInMemoryRpcBackend();

    cy.mount(
      <ConnectionProviders registry={aRegistryServing([THIS_HOST], backend)}>
        <ResolvedProviderProbe hostId={THIS_HOST} />
      </ConnectionProviders>,
    );

    // Then the connection says which provider issued it
    byTestId("resolved-provider").should("have.text", "in-memory");
  });

  it("resolves every host to nothing when no provider is registered", () => {
    // Given an empty registry — a page before any provider registers, and the shape a desktop
    // app has with no LiveKit configuration and no IPC provider yet
    cy.mount(
      <ConnectionProviders registry={new ConnectionProviderRegistry()}>
        <ResolvedProviderProbe hostId={THIS_HOST} />
      </ConnectionProviders>,
    );

    // Then nothing is reachable and nothing throws
    byTestId("resolved-provider").should("have.text", "unreachable");
  });
});
