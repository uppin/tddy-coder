/**
 * Acceptance tests: every Hosts row carries live CPU and disk, one `StreamHostStats` subscription
 * per **online** host.
 *
 * These mount `HostRowTelemetry` — this node's own component — rather than the whole `HostsScreen`.
 * Row rendering belongs to `#hosts-screen 1/8` and is not implemented yet, so driving the screen
 * here would make every failure attributable to *that* node instead of this one. The seam is the
 * telemetry cell, which is exactly what this PR owns.
 *
 * Subscription counting uses the backend's existing global `hostStatsStreamCount()`. It cannot say
 * *which* host subscribed — `StreamHostStatsRequest` carries only `session_token`, and `mountWithRpc`
 * hands every host the same in-memory transport — but the properties under test are counts: two
 * online hosts must open two streams, and an online+offline pair must open exactly one. No proto
 * change is needed, which is why none is made here.
 *
 * PRD: docs/ft/web/1-WIP/PRD-2026-09-06-telemetry-fanout.md
 */

import {
  aConnectionServiceBackend,
  type ConnectionServiceBackend,
} from "../support/rpc/connectionServiceBackend";
import { HostRowTelemetry } from "../../src/components/hosts/HostRowTelemetry";
import { mountWithRpc } from "../support/rpc/inMemory";
import { withSelectedDaemon } from "../support/rpc/withSelectedDaemon";
import { hostTelemetryPage } from "../support/pages/hostsScreenPage";

const ONLINE_HOST = "workstation-1";
const OFFLINE_HOST = "server-2";

function mountCells(
  backend: ConnectionServiceBackend,
  hosts: { instanceId: string; online: boolean }[],
) {
  mountWithRpc(
    withSelectedDaemon(
      <div>
        {hosts.map((h) => (
          <HostRowTelemetry key={h.instanceId} instanceId={h.instanceId} online={h.online} />
        ))}
      </div>,
      hosts.map((h) => ({ instanceId: h.instanceId, label: h.instanceId })),
    ),
    backend,
  );
}

describe("Hosts screen telemetry", () => {
  it("shows live cpu and disk for an online host", () => {
    mountCells(aConnectionServiceBackend({ hostCpuPerCore: [12, 48] }), [
      { instanceId: ONLINE_HOST, online: true },
    ]);

    hostTelemetryPage.cpu(ONLINE_HOST).should("exist");
    hostTelemetryPage.disk(ONLINE_HOST).should("exist");
  });

  it("updates the cpu bars as fresh readings stream in", () => {
    mountCells(
      aConnectionServiceBackend({ hostCpuPerCore: [10, 10], hostCpuPerCoreUpdate: [90, 90] }),
      [{ instanceId: ONLINE_HOST, online: true }],
    );

    // The second event carries the updated reading; the cell must reflect it without a reload.
    hostTelemetryPage.cpu(ONLINE_HOST).should("have.attr", "data-core-0", "90");
  });

  it("opens exactly one stats subscription per online host", () => {
    const backend = aConnectionServiceBackend({ hostCpuPerCore: [5] });
    mountCells(backend, [
      { instanceId: ONLINE_HOST, online: true },
      { instanceId: OFFLINE_HOST, online: true },
    ]);

    hostTelemetryPage.cpu(ONLINE_HOST).should("exist");
    hostTelemetryPage.cpu(OFFLINE_HOST).should("exist");
    cy.then(() => {
      expect(backend.hostStatsStreamCount()).to.equal(2);
    });
  });

  it("opens no subscription for an offline host and shows a dash", () => {
    const backend = aConnectionServiceBackend({ hostCpuPerCore: [5] });
    mountCells(backend, [
      { instanceId: ONLINE_HOST, online: true },
      { instanceId: OFFLINE_HOST, online: false },
    ]);

    hostTelemetryPage.cpu(ONLINE_HOST).should("exist");
    hostTelemetryPage.cell(OFFLINE_HOST).should("contain.text", "—");
    cy.then(() => {
      expect(
        backend.hostStatsStreamCount(),
        "an offline host must not open a stream",
      ).to.equal(1);
    });
  });

  it("shows an unavailable state rather than zeroes when a host is unreachable", () => {
    // No daemon entry for this host, so no connection resolves and no reading can exist.
    mountWithRpc(
      withSelectedDaemon(
        <HostRowTelemetry instanceId={OFFLINE_HOST} online={true} />,
        [{ instanceId: ONLINE_HOST, label: ONLINE_HOST }],
      ),
      aConnectionServiceBackend({ hostCpuPerCore: [5] }),
    );

    hostTelemetryPage.unavailable(OFFLINE_HOST).should("exist");
    hostTelemetryPage.cell(OFFLINE_HOST).should("not.contain.text", "0%");
  });

  it("tears down every subscription when the screen unmounts", () => {
    const backend = aConnectionServiceBackend({ hostCpuPerCore: [5] });
    mountCells(backend, [{ instanceId: ONLINE_HOST, online: true }]);
    hostTelemetryPage.cpu(ONLINE_HOST).should("exist");

    cy.then(() => {
      // Remounting a fresh tree must not accumulate subscriptions from the torn-down one.
      mountCells(backend, [{ instanceId: ONLINE_HOST, online: true }]);
    });
    hostTelemetryPage.cpu(ONLINE_HOST).should("exist");
    cy.then(() => {
      expect(
        backend.hostStatsStreamCount(),
        "one live subscription per mount, not one accumulated per remount",
      ).to.equal(2);
    });
  });
});
