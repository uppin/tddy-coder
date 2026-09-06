/**
 * Cypress component acceptance: per-host telemetry on the Hosts screen.
 *
 * Each cell shows one host's live per-core CPU and free disk, from one `StreamHostStats`
 * subscription per **online, routable** host.
 *
 * These mount `HostRowTelemetry` — this node's own component — rather than the whole `HostsScreen`.
 * Row rendering belongs to `#hosts-screen 1/8` and is not implemented, so driving the screen here
 * would make every failure attributable to *that* node instead of this one.
 *
 * **What this file deliberately does not test.** Two properties are invisible through the fake and
 * are pinned by unit tests instead, so nothing here pretends to cover them:
 *
 * - *Which* host a row reads (`src/components/hosts/hostTelemetryState.test.ts`). Every host shares
 *   one in-memory transport and the request carries only a session token, so a cell reading the
 *   selected daemon for every row renders identical DOM and opens an identical number of streams.
 * - Tearing a subscription down (`src/rpc/hostStatsSubscription.test.ts`). `createRouterTransport`
 *   propagates neither an abort nor a consumer's `break` to the server handler, so no backend
 *   counter here can fall back when a stream closes.
 *
 * PRD: docs/ft/web/1-WIP/PRD-2026-09-06-telemetry-fanout.md
 */

import {
  aConnectionServiceBackend,
  type ConnectionServiceBackend,
  type ConnectionServiceScenario,
} from "../support/rpc/connectionServiceBackend";
import { HostRowTelemetry } from "../../src/components/hosts/HostRowTelemetry";
import { mountWithRpc } from "../support/rpc/inMemory";
import { withSelectedDaemon } from "../support/rpc/withSelectedDaemon";
import { hostTelemetryPage as telemetry } from "../support/pages/hostsScreenPage";

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

const HOST_A = "workstation-1";
const HOST_B = "server-2";

const DISK = {
  availableBytes: 42_100_000_000n, // formats as "42.1 GB"
  totalBytes: 100_000_000_000n,
  projectDir: "/home/dev/repos",
};

const CPU_PER_CORE = [12, 48];
const CPU_AFTER_UPDATE = [90, 90];

function aTelemetryBackend(
  overrides: Partial<ConnectionServiceScenario> = {},
): ConnectionServiceBackend {
  return aConnectionServiceBackend({ hostDisk: DISK, hostCpuPerCore: CPU_PER_CORE, ...overrides });
}

type Row = { instanceId: string; online: boolean };

/**
 * Mount one telemetry cell per row. `reachable` names the hosts a wire can route to — by default
 * every mounted row, so a test opts into unreachability by naming a shorter list.
 */
function mountCells(backend: ConnectionServiceBackend, rows: Row[], reachable: string[] = rows.map((r) => r.instanceId)) {
  mountWithRpc(
    withSelectedDaemon(
      <div>
        {rows.map((row) => (
          <HostRowTelemetry key={row.instanceId} instanceId={row.instanceId} online={row.online} />
        ))}
      </div>,
      reachable.map((instanceId) => ({ instanceId, label: instanceId })),
    ),
    backend,
  );
}

function anOnlineRow(instanceId: string): Row {
  return { instanceId, online: true };
}

function anOfflineRow(instanceId: string): Row {
  return { instanceId, online: false };
}

// ---------------------------------------------------------------------------

describe("Hosts screen telemetry", () => {
  beforeEach(() => {
    cy.clearLocalStorage();
    cy.clearAllSessionStorage();
  });

  it("shows an online host's per-core cpu as it is streamed", () => {
    // Given a host reporting two cores at 12% and 48%
    const backend = aTelemetryBackend();

    // When its row is mounted
    mountCells(backend, [anOnlineRow(HOST_A)]);

    // Then both cores read as the daemon reported them
    telemetry.expectCpuCores(HOST_A, CPU_PER_CORE);
  });

  it("shows an online host's free disk as it is streamed", () => {
    // Given a host reporting 42.1 GB free
    const backend = aTelemetryBackend();

    // When its row is mounted
    mountCells(backend, [anOnlineRow(HOST_A)]);

    // Then the figure is the one an operator would read, not a zero
    telemetry.expectFreeDisk(HOST_A, "42.1 GB");
  });

  it("applies a fresher reading without a reload", () => {
    // Given a host whose second event carries both cores at 90%
    const backend = aTelemetryBackend({ hostCpuPerCoreUpdate: CPU_AFTER_UPDATE });

    // When its row is mounted
    mountCells(backend, [anOnlineRow(HOST_A)]);

    // Then the cell settles on the later reading
    telemetry.expectCpuCores(HOST_A, CPU_AFTER_UPDATE);
  });

  it("opens one subscription per online host", () => {
    // Given two online hosts
    const backend = aTelemetryBackend();

    // When both rows are mounted
    mountCells(backend, [anOnlineRow(HOST_A), anOnlineRow(HOST_B)]);
    telemetry.cpu(HOST_A).should("exist");
    telemetry.cpu(HOST_B).should("exist");

    // Then each opened exactly one, rather than one shared or one per render
    cy.then(() => {
      expect(backend.hostStatsStreamCount(), "one subscription per online host").to.equal(2);
    });
  });

  it("opens no subscription for an offline host", () => {
    // Given one online host beside one that has left the roster
    const backend = aTelemetryBackend();

    // When both rows are mounted
    mountCells(backend, [anOnlineRow(HOST_A), anOfflineRow(HOST_B)]);
    telemetry.cpu(HOST_A).should("exist");

    // Then only the online host is subscribed — an offline host has nothing to subscribe through
    cy.then(() => {
      expect(backend.hostStatsStreamCount(), "an offline host must not open a stream").to.equal(1);
    });
  });

  it("marks an offline host as having no reading", () => {
    // Given a host that has left the roster
    const backend = aTelemetryBackend();

    // When its row is mounted
    mountCells(backend, [anOfflineRow(HOST_B)]);

    // Then it says so in its own right, distinguishably from a reachable host with no disk figure
    telemetry.offline(HOST_B).should("exist");
    telemetry.expectNoReading(HOST_B);
  });

  it("marks an online host that nothing routes to as unavailable", () => {
    // Given an online host absent from the set of hosts a wire reaches
    const backend = aTelemetryBackend();

    // When its row is mounted
    mountCells(backend, [anOnlineRow(HOST_B)], [HOST_A]);

    // Then the cell says the host cannot be reached
    telemetry.unavailable(HOST_B).should("exist");
  });

  it("shows no reading at all for a host that nothing routes to", () => {
    // Given an online host nothing can reach
    const backend = aTelemetryBackend();

    // When its row is mounted
    mountCells(backend, [anOnlineRow(HOST_B)], [HOST_A]);
    telemetry.unavailable(HOST_B).should("exist");

    // Then neither metric is rendered — a zeroed bar is a reading an operator would act on
    telemetry.expectNoReading(HOST_B);
  });

  it("shows a pending marker for a subscribed host that has not reported yet", () => {
    // Given a host whose feed is open but silent
    const backend = aTelemetryBackend({ hostStatsSilent: true });

    // When its row is mounted
    mountCells(backend, [anOnlineRow(HOST_A)]);

    // Then the cell waits visibly rather than drawing idle bars
    telemetry.pending(HOST_A).should("exist");
    telemetry.expectNoReading(HOST_A);
  });
});
