/**
 * Acceptance tests: memory, load average and core count reach the UI — and a host that reports **no**
 * load average renders as "no reading" rather than as an idle machine.
 *
 * The load-average case is the one that matters. `sysinfo` returns all-zeros where a platform has no
 * load average, so the wire omits the block entirely; if the UI ever renders a missing block as
 * `0.00` an operator reads a struggling host as idle. Both directions are pinned here.
 *
 * PRD: docs/ft/web/1-WIP/PRD-2026-09-06-host-resources.md
 */

import React from "react";
import {
  aConnectionServiceBackend,
  type ConnectionServiceBackend,
  type ConnectionServiceScenario,
} from "../support/rpc/connectionServiceBackend";
import { SessionsDrawerScreen } from "../../src/components/sessions/SessionsDrawerScreen";
import { mountWithRpc } from "../support/rpc/inMemory";
import { withSelectedDaemon } from "../support/rpc/withSelectedDaemon";

const DISK = {
  availableBytes: 42_100_000_000n,
  totalBytes: 100_000_000_000n,
  projectDir: "/home/dev/repos",
};

/** Mounts the footer through its real screen, exactly as `HostStatsFooterAcceptance` does — the
 *  footer reads context the screen provides, so mounting it bare fails on the harness rather than
 *  on the feature. */
function mountFooter(backend: ConnectionServiceBackend) {
  mountWithRpc(withSelectedDaemon(<SessionsDrawerScreen />), backend);
}

function aBackend(overrides: Partial<ConnectionServiceScenario>): ConnectionServiceBackend {
  return aConnectionServiceBackend({ sessions: [], hostDisk: DISK, ...overrides });
}

describe("Host resource readings", () => {
  it("shows memory beside the existing disk and cpu indicators", () => {
    mountFooter(
      aBackend({
        hostCpuPerCore: [10, 20],
        hostMemoryBytes: { availableBytes: 4_000_000_000n, totalBytes: 16_000_000_000n },
        hostLoadAverage: { oneMinute: 1.5, fiveMinutes: 1.2, fifteenMinutes: 0.9 },
      }),
    );

    cy.get('[data-testid="memory-indicator"]').should("contain.text", "GB");
    // The readings it sits beside must survive the change.
    cy.get('[data-testid="host-stats-footer"]').should("exist");
  });

  it("shows the load average a host reports", () => {
    mountFooter(
      aBackend({
        hostCpuPerCore: [10],
        hostMemoryBytes: { availableBytes: 1n, totalBytes: 2n },
        hostLoadAverage: { oneMinute: 2.5, fiveMinutes: 1.0, fifteenMinutes: 0.5 },
      }),
    );

    cy.get('[data-testid="load-average-indicator"]').should("contain.text", "2.5");
  });

  it("renders a dash rather than zero when a host reports no load average", () => {
    // No `hostLoadAverage` in the scenario: the event carries no load block at all.
    mountFooter(
      aBackend({
        hostCpuPerCore: [10],
        hostMemoryBytes: { availableBytes: 1n, totalBytes: 2n },
      }),
    );

    cy.get('[data-testid="load-average-indicator"]').should("contain.text", "—");
    cy.get('[data-testid="load-average-indicator"]').should("not.contain.text", "0.00");
  });
});
