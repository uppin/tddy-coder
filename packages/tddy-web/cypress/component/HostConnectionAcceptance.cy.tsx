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
 * The "no LiveKit" guarantee here is structural rather than a spy: the only provider that can reach
 * a host is an in-memory one, and no `Room` is ever constructed, so there is nothing that *could*
 * carry the traffic over LiveKit. A spy would prove less and break the moment the LiveKit provider
 * moved file.
 *
 * Changeset: `docs/dev/1-WIP/2026-09-05-optional-livekit-connection-model.md`
 * Stack: `optional-livekit` node 1 of 7.
 */

import React from "react";
import { createClient, type Client } from "@connectrpc/connect";
import type { DescService } from "@bufbuild/protobuf";
import { create } from "@bufbuild/protobuf";
import { anInMemoryRpcBackend, type InMemoryRpcBackend } from "tddy-connectrpc-testkit";
import {
  ConnectionService,
  ListProjectBranchesResponseSchema,
  ListProjectsResponseSchema,
  ListSessionsResponseSchema,
  ProjectEntrySchema,
  SessionEntrySchema,
  type ProjectEntry,
} from "../../src/gen/connection_pb";
import { ProjectsAppPage } from "../../src/components/projects/ProjectsAppPage";
import { AuthProvider } from "../../src/hooks/authProvider";
import type { DaemonHost } from "../../src/lib/participantRole";
import {
  ConnectionProviders,
  ConnectionProviderRegistry,
  useHostClient,
  useHostConnection,
} from "../../src/rpc/connections/registry";
import { SelectedDaemonProvider } from "../../src/rpc/selectedDaemon";
import type { ConnectionProvider, HostConnection } from "../../src/rpc/connections/types";
import { mountWithRpc } from "../support/rpc/inMemory";
import { hostConnectionProbePage as page } from "../support/pages/hostConnectionProbePage";
import { projectsScreenPage } from "../support/pages/projectsScreenPage";
import { TEST_IDS } from "../support/testIds";

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

const THIS_HOST = "instance-this-host";
const A_PEER = "instance-a-peer";

/**
 * A provider serving `hosts` over `backend`, with no wire of its own.
 *
 * Deliberately **hostile to the hooks**: a *fresh* `HostConnection` is built on every
 * `connectHost` call, so nothing about this fixture makes a client identity stable. Caching one
 * connection per host here would have made "the same client across renders" a property of the test
 * double — the spec below would stay green with both memos in `registry.tsx` deleted. Client caching
 * still lives *inside* each connection, because that half is `HostConnection`'s own published
 * contract (and is what `LiveKitHostConnection` implements); what the spec has to prove is that the
 * hooks hand back the same *connection* to begin with.
 */
function anInMemoryProviderNamed(
  id: string,
  hosts: string[],
  backend: InMemoryRpcBackend,
): ConnectionProvider {
  const transport = backend.transport();
  return {
    id,
    connectHost: (hostId) => {
      if (!hosts.includes(hostId)) return null;
      // One client per service per connection, so client identity is as stable as the *connection* —
      // the guarantee `SessionClientCache` gives today, which a consumer keying an effect on the
      // client depends on. Nothing above this line is cached.
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
        openSession: () => {
          throw new Error("this provider serves host-level RPC only; sessions are node 3's");
        },
      };
      return connection;
    },
  };
}

/** A registry holding one in-memory provider for `hosts`. No LiveKit provider is ever registered. */
function aRegistryServing(hosts: string[], backend: InMemoryRpcBackend): ConnectionProviderRegistry {
  const registry = new ConnectionProviderRegistry();
  registry.register(anInMemoryProviderNamed("in-memory", hosts, backend));
  return registry;
}

// ---------------------------------------------------------------------------
// Probes
// ---------------------------------------------------------------------------

/**
 * Lists a host's sessions through whatever wire the registry resolved.
 *
 * The initial label is `"resolving"` and *not* `"no client"`, which the null branch of the effect
 * sets: Cypress stops retrying the moment an assertion first passes, so a probe whose initial state
 * were also `"no client"` would satisfy the two "no client" specs below at first paint, before the
 * hooks had answered anything at all. `"no client"` has to be reachable only through the effect for
 * those assertions to mean what they say.
 */
function SessionCountProbe({ hostId }: { hostId: string | null }) {
  const client = useHostClient(ConnectionService, hostId);
  const [label, setLabel] = React.useState("resolving");

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

  return <div data-testid={TEST_IDS.hostConnectionSessionCount}>{label}</div>;
}

/**
 * Renders how many distinct client instances this component has been handed, and how many times it
 * rendered to be handed them.
 *
 * The render count is pinned alongside the identity count because "one distinct client" is only
 * evidence of stability if the component actually re-rendered: three clicks that silently did
 * nothing would otherwise leave the assertion green.
 */
function ClientIdentityProbe({ hostId }: { hostId: string | null }) {
  const client = useHostClient(ConnectionService, hostId);
  const seen = React.useRef(new Set<unknown>());
  const renders = React.useRef(0);
  const [, forceRender] = React.useState(0);
  renders.current += 1;
  if (client) seen.current.add(client);
  return (
    <div>
      <div data-testid={TEST_IDS.hostConnectionDistinctClients}>{seen.current.size}</div>
      <div data-testid={TEST_IDS.hostConnectionRenderCount}>{renders.current}</div>
      <button data-testid={TEST_IDS.hostConnectionReRender} onClick={() => forceRender((n) => n + 1)}>
        render again
      </button>
    </div>
  );
}

/** Names the provider that answered for `hostId`, or says nothing could reach it. */
function ResolvedProviderProbe({ hostId }: { hostId: string | null }) {
  const connection = useHostConnection(hostId);
  return (
    <div data-testid={TEST_IDS.hostConnectionResolvedProvider}>
      {connection?.providerId ?? "unreachable"}
    </div>
  );
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
    page.shouldHaveListedSessions(2);
  });

  it("hands back the same client instance across renders while the host is unchanged", () => {
    // Given a resolvable host, served by a provider that hands out a brand-new connection object on
    // every single `connectHost` call — so any stability observed below is the hooks' own
    const backend = anInMemoryRpcBackend();

    cy.mount(
      <ConnectionProviders registry={aRegistryServing([THIS_HOST], backend)}>
        <ClientIdentityProbe hostId={THIS_HOST} />
      </ConnectionProviders>,
    );

    // When the component re-renders several times without its host changing
    page.reRender(3);

    // Then it did re-render — otherwise the count below proves nothing …
    page.shouldHaveRendered(4);

    // … and across those renders it was handed one client, not four. Consumers key effects on the
    // client — the Agent Activity feed in `useAcpReplay` is one — and a fresh instance per render
    // tears their stream down and cancels the snapshot pull in flight.
    page.shouldHaveBeenHandedDistinctClients(1);
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
    page.shouldHaveNoClient();
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
    page.shouldHaveNoClient();
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
    page.shouldHaveResolvedThrough("in-memory");
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
    page.shouldBeUnreachable();
  });
});

// ---------------------------------------------------------------------------
// The headline claim, on a real screen
// ---------------------------------------------------------------------------

const PROJECT_HOST = "workstation-1";

const DAEMON_HOSTS: DaemonHost[] = [{ instanceId: PROJECT_HOST, label: "workstation-1 (this daemon)" }];

function aProject(overrides: Partial<ProjectEntry>): ProjectEntry {
  return create(ProjectEntrySchema, {
    projectId: "proj-alpha",
    name: "alpha",
    gitUrl: "https://example.com/alpha.git",
    mainRepoPath: "/home/dev/repos/alpha",
    daemonInstanceId: PROJECT_HOST,
    mainBranchRef: "",
    defaultRemote: "",
    ...overrides,
  });
}

/**
 * The Projects screen is the smallest real daemon-level screen: its whole data path is
 * `useDaemonClient(ConnectionService)` — that is, `useHostClient` — plus a `useHostConnector` for the
 * host an operator picks. Nothing else about it is in this node's way.
 *
 * `room={null}` is the point of the mount. `SelectedDaemonProvider` still owns the host *directory*
 * in this node (node 2 takes that over), so it is still what renders `LiveKitConnections` — but with
 * no room, no `livekit-client` `Room` is ever constructed and the LiveKit provider claims not one
 * host. The only wire that can reach `PROJECT_HOST` is the in-memory provider registered above it,
 * which is exactly the desktop app's shape: LiveKit present in the bundle, absent from the wiring.
 */
function aProjectsScreenReachingItsHostWithoutLiveKit(registry: ConnectionProviderRegistry) {
  return (
    <AuthProvider>
      <ConnectionProviders registry={registry}>
        <SelectedDaemonProvider room={null} daemons={DAEMON_HOSTS} servingInstanceId={PROJECT_HOST}>
          <ProjectsAppPage onNavigate={cy.stub()} />
        </SelectedDaemonProvider>
      </ConnectionProviders>
    </AuthProvider>
  );
}

describe("a daemon-level screen with no LiveKit room in the tree", () => {
  beforeEach(() => {
    cy.viewport(1280, 800);
    cy.clearLocalStorage();
    cy.clearAllSessionStorage();
    window.localStorage.setItem("tddy_session_token", "fake-token");
  });

  it("loads the Projects screen's data over an in-memory provider", () => {
    // Given the Projects screen's list RPC served by an in-memory backend, reachable only through a
    // connection provider that knows nothing about LiveKit
    const backend = anInMemoryRpcBackend()
      .onUnary(ConnectionService.method.listProjects, () =>
        create(ListProjectsResponseSchema, { projects: [aProject({ projectId: "proj-alpha" })] }),
      )
      .onUnary(ConnectionService.method.listProjectBranches, () =>
        create(ListProjectBranchesResponseSchema, { branches: [], defaultRemote: "origin" }),
      );

    // When the screen is mounted with no common room joined
    mountWithRpc(
      aProjectsScreenReachingItsHostWithoutLiveKit(aRegistryServing([PROJECT_HOST], backend)),
      backend,
    );

    // Then it renders its host's projects. This is the claim the whole node exists for: a
    // daemon-level screen reaching a daemon with no `Room` in the tree — before this node the same
    // screen could reach nothing at all without one.
    projectsScreenPage.card("proj-alpha").should("exist");
    projectsScreenPage.hostRowDaemonIds("proj-alpha").should("deep.equal", [PROJECT_HOST]);
    cy.wrap(backend).should((b) => {
      expect(b.callsTo(ConnectionService.method.listProjects)).to.have.length.greaterThan(0);
    });
  });

  it("renders the same screen empty, and without failing, when nothing can reach the host", () => {
    // Given the same screen with an empty registry — no LiveKit, and no other wire either, which is
    // what a desktop build looks like before node 6 registers its IPC provider
    const backend = anInMemoryRpcBackend().onUnary(ConnectionService.method.listProjects, () =>
      create(ListProjectsResponseSchema, { projects: [aProject({ projectId: "proj-alpha" })] }),
    );

    // When it is mounted
    mountWithRpc(
      aProjectsScreenReachingItsHostWithoutLiveKit(new ConnectionProviderRegistry()),
      backend,
    );

    // Then the screen is there and empty rather than broken — the null guard every call site
    // already had is what makes a missing wire an ordinary state — and nothing was asked of a
    // backend nothing could reach
    projectsScreenPage.screen().should("exist");
    projectsScreenPage.card("proj-alpha").should("not.exist");
    cy.wrap(backend).should((b) => {
      expect(b.callsTo(ConnectionService.method.listProjects)).to.have.length(0);
    });
  });
});
