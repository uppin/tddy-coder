/**
 * Cypress component acceptance: the Hosts screen loads through **`host.HostService`**.
 *
 * `ListKnownHosts`, `GetHostTooling` and `StreamHostStats` used to be `connection.ConnectionService`
 * methods; they are now `host.HostService`'s. The screen therefore has to address the new service,
 * and this spec is what says so: its backend implements `host.HostService` and **nothing else**, so
 * a screen still bound to `ConnectionService` gets `Unimplemented` and renders its error instead of
 * a table. Reading only the rendered rows would not distinguish the two — a fixture served under
 * either coordinate produces the same table — which is why the recorded call is asserted too.
 *
 * Changeset: `docs/dev/1-WIP/2026-09-09-unbundle-host-worktree-services.md`
 */

import { anInMemoryRpcBackend, type InMemoryRpcBackend } from "tddy-connectrpc-testkit";
import { HostsAppPage } from "../../src/components/hosts/HostsAppPage";
import { HostService, type KnownHostEntry } from "../../src/gen/host_pb";
import { aHostServiceFake } from "../support/rpc/hostServiceBackend";
import { mountWithRpc } from "../support/rpc/inMemory";
import { withSelectedDaemon } from "../support/rpc/withSelectedDaemon";
import { hostsScreenPage, hostTelemetryPage as telemetry } from "../support/pages/hostsScreenPage";

const HOST = "workstation-1";
const CPU_PER_CORE = [12, 34];
const DISK = { availableBytes: 500n * 1024n ** 3n, totalBytes: 1000n * 1024n ** 3n, projectDir: "/repos" };

function aKnownHost(overrides: Partial<KnownHostEntry>): KnownHostEntry {
  return {
    instanceId: HOST,
    label: HOST,
    online: true,
    firstSeenUnixMs: BigInt(Date.now()),
    lastSeenUnixMs: BigInt(Date.now()),
    reposBasePath: "repos",
    maxAttachmentBytes: 0n,
    isLocal: false,
    ...overrides,
  } as KnownHostEntry;
}

/**
 * A daemon that answers on `host.HostService` and on no other service.
 *
 * The registry listing is composed with `aHostServiceFake`'s telemetry handlers so the whole screen
 * — rows and live readings — is served from one service, which is the claim under test.
 */
function aDaemonServingOnlyHostService(hosts: KnownHostEntry[]): InMemoryRpcBackend {
  return anInMemoryRpcBackend().implement(HostService, {
    ...aHostServiceFake({ hostCpuPerCore: CPU_PER_CORE, hostDisk: DISK }).handlers,
    listKnownHosts: () => ({ hosts }),
  });
}

describe("Hosts screen over host.HostService", () => {
  it("lists the known hosts a daemon serving only host.HostService reports", () => {
    // Given a daemon that answers ListKnownHosts under `host.HostService` and serves no
    // `connection.ConnectionService` at all
    const backend = aDaemonServingOnlyHostService([aKnownHost({})]);

    // When the Hosts screen is opened
    mountWithRpc(withSelectedDaemon(<HostsAppPage onNavigate={() => {}} />, [
      { instanceId: HOST, label: HOST },
    ]), backend);

    // Then the host is on screen, and the read that put it there was addressed to the new service
    hostsScreenPage.row(HOST).should("exist");
    cy.then(() => {
      expect(backend.callsTo(HostService.method.listKnownHosts)).to.have.length(1);
    });
  });

  it("shows a row's live reading from the same service's StreamHostStats", () => {
    // Given the same daemon, streaming that host's per-core utilization
    const backend = aDaemonServingOnlyHostService([aKnownHost({ online: true })]);

    // When the Hosts screen is opened
    mountWithRpc(withSelectedDaemon(<HostsAppPage onNavigate={() => {}} />, [
      { instanceId: HOST, label: HOST },
    ]), backend);

    // Then the row carries the reading the host streamed — the telemetry feed moved with the
    // registry listing, so a screen that split them across two services would show one and not
    // the other
    hostsScreenPage.row(HOST).within(() => {
      telemetry.expectCpuCores(HOST, CPU_PER_CORE);
    });
  });
});
