/**
 * Acceptance tests: each host row reports whether a desktop is reachable and whether tddy could
 * bridge it — as two separate facts.
 *
 * The distinguishing test is the point. "tddy cannot bridge here" and "nothing is serving a desktop
 * here" have unrelated fixes (install the bridge / start a server), so a row that renders them the
 * same sends an operator to the wrong one.
 *
 * PRD: docs/ft/web/1-WIP/PRD-2026-09-06-desktop-probe.md
 */

import { create } from "@bufbuild/protobuf";
import { anInMemoryRpcBackend } from "tddy-connectrpc-testkit";
import { HostRemoteDesktopSchema, ProbeOutcome } from "../../src/gen/connection_pb";
import { HostRowRemoteDesktop } from "../../src/components/hosts/HostRowRemoteDesktop";
import { mountWithRpc } from "../support/rpc/inMemory";
import { withSelectedDaemon } from "../support/rpc/withSelectedDaemon";
import { hostRemoteDesktopPage } from "../support/pages/hostsScreenPage";

const HOST = "workstation-1";
const VNC = 1;
const RDP = 2;

function aReading(overrides: Record<string, unknown>) {
  return create(HostRemoteDesktopSchema, {
    outcome: ProbeOutcome.OK,
    protocol: VNC,
    canBridge: true,
    desktopReachable: true,
    port: 5900,
    ...overrides,
  });
}

function mountReadings(readings: ReturnType<typeof aReading>[]) {
  mountWithRpc(
    withSelectedDaemon(<HostRowRemoteDesktop instanceId={HOST} readings={readings} />),
    anInMemoryRpcBackend(),
  );
}

describe("Hosts screen remote desktop", () => {
  it("shows vnc and rdp reachability for a host", () => {
    mountReadings([
      aReading({ protocol: VNC, desktopReachable: true, port: 5900 }),
      aReading({ protocol: RDP, desktopReachable: false, port: 3389 }),
    ]);

    hostRemoteDesktopPage.protocol(HOST, "vnc").should("exist");
    hostRemoteDesktopPage.protocol(HOST, "rdp").should("exist");
  });

  it("distinguishes a host that cannot bridge from one with no desktop serving", () => {
    // A desktop is serving, but this daemon has no bridge binary for it.
    mountReadings([aReading({ protocol: VNC, canBridge: false, desktopReachable: true })]);
    hostRemoteDesktopPage.protocol(HOST, "vnc").should("contain.text", "No bridge");

    // The bridge is present, but nothing is serving a desktop.
    mountReadings([aReading({ protocol: VNC, canBridge: true, desktopReachable: false })]);
    hostRemoteDesktopPage.protocol(HOST, "vnc").should("contain.text", "No desktop");
    hostRemoteDesktopPage.protocol(HOST, "vnc").should("not.contain.text", "No bridge");
  });

  it("names the port that was checked when reporting unreachable", () => {
    mountReadings([aReading({ protocol: VNC, desktopReachable: false, port: 5901 })]);

    // Without the port, "unreachable" reads as authoritative for a host that simply serves elsewhere.
    hostRemoteDesktopPage.protocol(HOST, "vnc").should("contain.text", "5901");
  });

  it("distinguishes a failed probe from a negative finding", () => {
    mountReadings([
      aReading({
        protocol: VNC,
        outcome: ProbeOutcome.FAILED,
        desktopReachable: false,
        failureReason: "connect timed out",
      }),
    ]);

    hostRemoteDesktopPage.protocol(HOST, "vnc").should("contain.text", "Could not check");
    hostRemoteDesktopPage.protocol(HOST, "vnc").should("not.contain.text", "No desktop");
  });
});
