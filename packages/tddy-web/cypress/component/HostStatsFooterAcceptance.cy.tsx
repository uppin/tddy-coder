/**
 * Cypress component acceptance: Host Stats Footer.
 *
 * A screen-level bottom strip on `SessionsDrawerScreen` that hosts the relocated byte-traffic
 * readout plus two host-level indicators — available disk space and per-core CPU usage — for the
 * currently selected daemon.
 *
 * PRD: `docs/ft/web/1-WIP/PRD-2026-07-22-streamed-host-stats.md`
 * Changeset: `2026-07-22-streamed-host-stats`
 *
 * Host stats are sourced from a single `ConnectionService.StreamHostStats` server-stream over the
 * daemon client, stubbed by the in-memory backend. The disk fixture is 42.1 GB free of a 100 GB
 * filesystem; the CPU fixture is four logical cores at 10 / 55 / 90 / 30 %.
 */

import React from "react";
import { SessionsDrawerScreen } from "../../src/components/sessions/SessionsDrawerScreen";
import { withSelectedDaemon } from "../support/rpc/withSelectedDaemon";
import { mountWithRpc } from "../support/rpc/inMemory";
import {
  aConnectionServiceBackend,
  type ConnectionServiceBackend,
  type ConnectionServiceScenario,
} from "../support/rpc/connectionServiceBackend";
import { hostStatsFooterPage as footer } from "../support/pages/hostStatsFooterPage";

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

const DISK = {
  availableBytes: 42_100_000_000n, // formats as "42.1 GB"
  totalBytes: 100_000_000_000n,
  projectDir: "/home/dev/repos",
};

const CPU_PER_CORE = [10, 55, 90, 30];

function aHostStatsBackend(
  overrides: Partial<ConnectionServiceScenario> = {},
): ConnectionServiceBackend {
  return aConnectionServiceBackend({
    sessions: [],
    hostDisk: DISK,
    hostCpuPerCore: CPU_PER_CORE,
    ...overrides,
  });
}

function mountScreen(backend: ConnectionServiceBackend) {
  mountWithRpc(withSelectedDaemon(<SessionsDrawerScreen />), backend);
}

// ---------------------------------------------------------------------------

describe("Host Stats Footer", () => {
  beforeEach(() => {
    cy.viewport(1280, 800);
    cy.clearLocalStorage();
    cy.clearAllSessionStorage();
    window.localStorage.setItem("tddy_session_token", "fake-token");
  });

  it("shows the selected daemon's available disk space in the bottom footer", () => {
    // Given a daemon streaming 42.1 GB free on the project-directory filesystem
    const backend = aHostStatsBackend();

    // When the sessions drawer screen is mounted
    mountScreen(backend);

    // Then the footer's disk readout shows that free space
    footer.footer().should("exist");
    footer.diskAvailable().should("contain.text", "42.1 GB").and("contain.text", "free");
  });

  it("renders one CPU mini bar per logical core, each encoding its utilization percentage", () => {
    // Given a daemon streaming four cores at 10 / 55 / 90 / 30 %
    const backend = aHostStatsBackend();

    // When the sessions drawer screen is mounted
    mountScreen(backend);

    // Then there is exactly one bar per core, in core order, each carrying its percentage
    footer.cpuCores().should("exist");
    footer.cpuCoreBars().should("have.length", 4);
    footer.cpuCoreBar(0).should("have.attr", "data-percent", "10");
    footer.cpuCoreBar(1).should("have.attr", "data-percent", "55");
    footer.cpuCoreBar(2).should("have.attr", "data-percent", "90");
    footer.cpuCoreBar(3).should("have.attr", "data-percent", "30");
  });

  it("relocates the byte-traffic readout into the bottom footer", () => {
    // Given any daemon
    const backend = aHostStatsBackend();

    // When the sessions drawer screen is mounted
    mountScreen(backend);

    // Then the byte-traffic strip lives inside the footer (no longer in the top header)
    footer.trafficStripInFooter().should("exist");
  });

  it("shows host stats even when no session is selected", () => {
    // Given a daemon with no sessions at all
    const backend = aHostStatsBackend({ sessions: [] });

    // When the sessions drawer screen is mounted (nothing selected)
    mountScreen(backend);

    // Then the host-level footer is still present with both indicators
    footer.footer().should("exist");
    footer.diskAvailable().should("contain.text", "42.1 GB");
    footer.cpuCoreBars().should("have.length", 4);
  });

  it("sources both indicators from a single StreamHostStats subscription", () => {
    // Given a daemon streaming both CPU and disk in one feed
    const backend = aHostStatsBackend();

    // When the sessions drawer screen is mounted and both indicators have rendered
    mountScreen(backend);
    footer.diskAvailable().should("contain.text", "42.1 GB");
    footer.cpuCoreBars().should("have.length", 4);

    // Then the footer opened exactly one host-stats stream (not two separate subscriptions)
    footer.footer().then(() => {
      expect(backend.hostStatsStreamCount()).to.equal(1);
    });
  });

  it("updates the CPU bars as fresh readings stream in", () => {
    // Given a daemon whose stream pushes a second reading of 20 / 60 / 95 / 35 % after the first
    const backend = aHostStatsBackend({ hostCpuPerCoreUpdate: [20, 60, 95, 35] });

    // When the sessions drawer screen is mounted
    mountScreen(backend);

    // Then the bars reflect the latest streamed reading
    footer.cpuCoreBars().should("have.length", 4);
    footer.cpuCoreBar(0).should("have.attr", "data-percent", "20");
    footer.cpuCoreBar(1).should("have.attr", "data-percent", "60");
    footer.cpuCoreBar(2).should("have.attr", "data-percent", "95");
    footer.cpuCoreBar(3).should("have.attr", "data-percent", "35");
  });
});
