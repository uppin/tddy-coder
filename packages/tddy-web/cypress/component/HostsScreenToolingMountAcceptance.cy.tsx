/**
 * Cypress component acceptance: the Hosts screen asks each online host for its tooling, and so
 * offers that host's desktop.
 *
 * Every other tooling spec mounts `HostRowTooling` or `HostRowRemoteDesktop` directly, handing them
 * readings — which is exactly why the feature was unreachable in the running app while every one of
 * them passed. These tests mount the **assembled screen** (`HostsAppPage`, fed by `ListKnownHosts`)
 * and let the row fetch its own `GetHostTooling`, so being on the row at all is what is under test.
 *
 * The wire is stated rather than implied: each reachable host is a `HostConnection` reached over
 * LiveKit (it carries `media`, which Connect requires), registered with `aRegistryServing`, and
 * `SelectedDaemonProvider` is given no room so that registry is the only answer. A host absent from
 * the registry has no wire at all.
 *
 * PRD: docs/ft/web/1-WIP/PRD-2026-10-18-host-desktops-hosts-mount.md
 */

import { create } from "@bufbuild/protobuf";
import { Code } from "@connectrpc/connect";
import { anInMemoryRpcBackend, type InMemoryRpcBackend } from "tddy-connectrpc-testkit";
import { HostsAppPage } from "../../src/components/hosts/HostsAppPage";
import {
  GetHostToolingResponseSchema,
  HostGitIdentitySchema,
  HostGithubCliSchema,
  HostRemoteDesktopSchema,
  HostService,
  ProbeOutcome,
  type KnownHostEntry,
} from "../../src/gen/host_pb";
import { AuthProvider } from "../../src/hooks/authProvider";
import { ConnectionProviders } from "../../src/rpc/connections/registry";
import { SelectedDaemonProvider } from "../../src/rpc/selectedDaemon";
import { mountWithRpc } from "../support/rpc/inMemory";
import { aHostConnection, aRegistryServing } from "../support/rpc/hostConnections";
import { aHostServiceFake } from "../support/rpc/hostServiceBackend";
import {
  hostDesktopPage,
  hostToolingCellPage,
  hostToolingPage,
  hostsScreenPage,
} from "../support/pages/hostsScreenPage";

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

const ONLINE_HOST = "workstation-1";
const OFFLINE_HOST = "server-2";
const VNC = 1;

function aKnownHost(instanceId: string, online: boolean): KnownHostEntry {
  return {
    instanceId,
    label: instanceId,
    online,
    firstSeenUnixMs: BigInt(Date.now() - 86_400_000),
    lastSeenUnixMs: BigInt(Date.now()),
    reposBasePath: "repos",
    maxAttachmentBytes: 0n,
    isLocal: false,
  } as KnownHostEntry;
}

const anOnlineHost = (instanceId: string) => aKnownHost(instanceId, true);
const anOfflineHost = (instanceId: string) => aKnownHost(instanceId, false);

/** A tooling report whose VNC desktop answers on 5900 and which this host can bridge. */
function aToolingReportWithAReachableVnc(instanceId: string) {
  return create(GetHostToolingResponseSchema, {
    daemonInstanceId: instanceId,
    git: create(HostGitIdentitySchema, {
      outcome: ProbeOutcome.OK,
      configured: true,
      userName: "Ada Lovelace",
      userEmail: "ada@example.com",
    }),
    githubCli: create(HostGithubCliSchema, {
      outcome: ProbeOutcome.OK,
      installed: true,
      authenticated: true,
      login: "ada",
    }),
    remoteDesktop: [
      create(HostRemoteDesktopSchema, {
        outcome: ProbeOutcome.OK,
        protocol: VNC,
        canBridge: true,
        desktopReachable: true,
        port: 5900,
      }),
    ],
  });
}

/**
 * A daemon that lists `hosts` and streams telemetry for them — the rest of a row's cells — but
 * serves no `GetHostTooling` until a scenario below adds one.
 */
function aDaemonListing(hosts: KnownHostEntry[]): InMemoryRpcBackend {
  return anInMemoryRpcBackend().implement(HostService, {
    ...aHostServiceFake({ hostCpuPerCore: [10] }).handlers,
    listKnownHosts: () => ({ hosts }),
  });
}

/** Every host answers `GetHostTooling` with a reachable, bridgeable VNC desktop. */
function aDaemonWhoseHostsReportAReachableVnc(hosts: KnownHostEntry[]): InMemoryRpcBackend {
  return aDaemonListing(hosts).onUnary(HostService.method.getHostTooling, (req) =>
    aToolingReportWithAReachableVnc(req.daemonInstanceId),
  );
}

/** Every host is asked `GetHostTooling` and none answers. */
function aDaemonWhoseHostsDoNotAnswerForTooling(hosts: KnownHostEntry[]): InMemoryRpcBackend {
  return aDaemonListing(hosts).failWith(
    HostService.method.getHostTooling,
    Code.Unavailable,
    "the host did not answer",
  );
}

/**
 * Open the Hosts screen with `reachable` as the hosts a media-carrying wire reaches — the first of
 * them is the daemon serving the page.
 */
function openTheHostsScreen(backend: InMemoryRpcBackend, reachable: string[]) {
  const connections = reachable.map((instanceId) =>
    aHostConnection(instanceId).reachedOverLiveKit().servingOver(backend.transport()).build(),
  );
  mountWithRpc(
    <AuthProvider>
      <ConnectionProviders registry={aRegistryServing(...connections)}>
        <SelectedDaemonProvider
          room={null}
          daemons={reachable.map((instanceId) => ({ instanceId, label: instanceId }))}
          servingInstanceId={reachable[0]}
        >
          <HostsAppPage onNavigate={() => {}} />
        </SelectedDaemonProvider>
      </ConnectionProviders>
    </AuthProvider>,
    backend,
  );
}

const toolingCallsTo = (backend: InMemoryRpcBackend) =>
  backend.callsTo(HostService.method.getHostTooling);

// ---------------------------------------------------------------------------

describe("Hosts screen tooling mount", () => {
  beforeEach(() => {
    cy.clearLocalStorage();
    cy.clearAllSessionStorage();
  });

  it("offers connect on an online hosts row", () => {
    // Given an online host whose VNC desktop is reachable and bridgeable, over a wire carrying media
    const backend = aDaemonWhoseHostsReportAReachableVnc([anOnlineHost(ONLINE_HOST)]);

    // When the Hosts screen is opened
    openTheHostsScreen(backend, [ONLINE_HOST]);

    // Then that host's own row offers a way into its desktop
    hostsScreenPage.row(ONLINE_HOST).within(() => {
      hostDesktopPage.connect(ONLINE_HOST).should("exist");
    });
  });

  it("asks an online host for its tooling once per visit", () => {
    // Given an online host that reports its tooling
    const backend = aDaemonWhoseHostsReportAReachableVnc([anOnlineHost(ONLINE_HOST)]);

    // When the Hosts screen is opened and the row has shown the report
    openTheHostsScreen(backend, [ONLINE_HOST]);
    hostToolingPage.gitIdentity(ONLINE_HOST).should("contain.text", "Ada Lovelace");

    // Then the host was asked exactly once, about itself
    cy.then(() => {
      const calls = toolingCallsTo(backend);
      expect(calls, "one GetHostTooling per online row per visit").to.have.length(1);
      expect(calls[0].daemonInstanceId).to.equal(ONLINE_HOST);
    });
  });

  it("asks an offline host for nothing", () => {
    // Given one online host beside one that has left the roster
    const backend = aDaemonWhoseHostsReportAReachableVnc([
      anOnlineHost(ONLINE_HOST),
      anOfflineHost(OFFLINE_HOST),
    ]);

    // When the Hosts screen is opened and the online row has shown its report
    openTheHostsScreen(backend, [ONLINE_HOST]);
    hostsScreenPage.row(OFFLINE_HOST).should("exist");
    hostToolingPage.gitIdentity(ONLINE_HOST).should("contain.text", "Ada Lovelace");

    // Then only the online host was asked — an offline host cannot answer
    cy.then(() => {
      expect(toolingCallsTo(backend), "an offline host must not be asked").to.have.length(1);
    });
  });

  it("says the tooling could not be read when the host does not answer", () => {
    // Given an online host that does not answer GetHostTooling
    const backend = aDaemonWhoseHostsDoNotAnswerForTooling([anOnlineHost(ONLINE_HOST)]);

    // When the Hosts screen is opened
    openTheHostsScreen(backend, [ONLINE_HOST]);

    // Then the row says its tooling could not be read, rather than leaving the cell blank
    hostsScreenPage.row(ONLINE_HOST).within(() => {
      hostToolingCellPage.expectCouldNotBeRead(ONLINE_HOST);
      hostDesktopPage.connect(ONLINE_HOST).should("not.exist");
    });
  });
});
