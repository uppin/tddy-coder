/**
 * Acceptance tests: opening a host's desktop from its row.
 *
 * The gating tests are the substance. This action cannot work without the `media` capability — a
 * frame pipe carries no video — so following `InspectorTabs` the control is **absent**, not
 * disabled. Asserting "not.exist" rather than "be.disabled" is the difference between a UI that is
 * honest about what a transport can do and one that offers something it cannot deliver.
 *
 * PRD: docs/ft/web/1-WIP/PRD-2026-09-06-desktop-connect.md
 */

import { create } from "@bufbuild/protobuf";
import { anInMemoryRpcBackend } from "tddy-connectrpc-testkit";
import { HostRemoteDesktopSchema, ProbeOutcome } from "../../src/gen/connection_pb";
import { HostRowRemoteDesktop } from "../../src/components/hosts/HostRowRemoteDesktop";
import { mountWithRpc } from "../support/rpc/inMemory";
import { withSelectedDaemon } from "../support/rpc/withSelectedDaemon";
import { aHostConnection, aRegistryServing } from "../support/rpc/hostConnections";
import { ConnectionProviders } from "../../src/rpc/connections/registry";
import { SelectedDaemonProvider } from "../../src/rpc/selectedDaemon";
import { AuthProvider } from "../../src/hooks/authProvider";
import { hostDesktopPage } from "../support/pages/hostsScreenPage";

const HOST = "workstation-1";
const VNC = 1;

function aReachableVnc() {
  return create(HostRemoteDesktopSchema, {
    outcome: ProbeOutcome.OK,
    protocol: VNC,
    canBridge: true,
    desktopReachable: true,
    port: 5900,
  });
}

/** Mounts the row section with a daemon list that carries media (the LiveKit path). */
function mountWithMedia(readings = [aReachableVnc()]) {
  mountWithRpc(
    withSelectedDaemon(<HostRowRemoteDesktop instanceId={HOST} readings={readings} />),
    anInMemoryRpcBackend(),
  );
}

/**
 * Mounts the row over a wire the test **states**, rather than one a fixture implies.
 *
 * `withSelectedDaemon` hands the tree a `Room`, and a room makes the LiveKit provider claim every
 * host with all three capabilities — so it cannot express the `{"rpc"}`-only half of a gating
 * scenario at all. `room={null}` plus a registry built from `aHostConnection` makes this
 * connection the only answer, which is the difference between asserting the gate and constructing
 * it. See `cypress/support/rpc/hostConnections.ts`.
 */
function mountOnWire(carriesMedia: boolean) {
  const backend = anInMemoryRpcBackend();
  const host = carriesMedia
    ? aHostConnection(HOST).reachedOverLiveKit().servingOver(backend.transport()).build()
    : aHostConnection(HOST).servingOver(backend.transport()).build();
  mountWithRpc(
    <AuthProvider>
      <ConnectionProviders registry={aRegistryServing(host)}>
        <SelectedDaemonProvider
          room={null}
          daemons={[{ instanceId: HOST, label: HOST }]}
          servingInstanceId={HOST}
        >
          <HostRowRemoteDesktop instanceId={HOST} readings={[aReachableVnc()]} />
        </SelectedDaemonProvider>
      </ConnectionProviders>
    </AuthProvider>,
    backend,
  );
}

describe("Host desktop connect", () => {
  it("offers a connect action for a reachable host on a media carrying connection", () => {
    // Given a host whose desktop is reachable and which tddy can bridge
    mountWithMedia();

    // When the row is rendered
    // Then it offers a way in
    hostDesktopPage.connect(HOST).should("exist");
  });

  it("offers connect for a reachable desktop but not for an unreachable one", () => {
    // Given a reachable desktop
    mountWithMedia();

    // Then the action is offered
    hostDesktopPage.connect(HOST).should("exist");

    // When the same host reports no desktop serving
    mountWithMedia([
      create(HostRemoteDesktopSchema, {
        outcome: ProbeOutcome.OK,
        protocol: VNC,
        canBridge: true,
        desktopReachable: false,
        port: 5900,
      }),
    ]);

    // Then it is withdrawn — asserted against the positive case, so an action that never renders
    // at all cannot satisfy this test.
    hostDesktopPage.connect(HOST).should("not.exist");
  });

  it("offers connect when tddy can bridge but not when the bridge is missing", () => {
    // Given a host that can bridge a reachable desktop
    mountWithMedia();
    hostDesktopPage.connect(HOST).should("exist");

    // When the same desktop is reachable but this daemon has no bridge binary
    mountWithMedia([
      create(HostRemoteDesktopSchema, {
        outcome: ProbeOutcome.OK,
        protocol: VNC,
        canBridge: false,
        desktopReachable: true,
        port: 5900,
      }),
    ]);

    // Then connecting is not offered: a reachable desktop tddy cannot stream is still not connectable
    hostDesktopPage.connect(HOST).should("not.exist");
  });

  it("offers connect over a media carrying connection but not over one without media", () => {
    // Given the same reachable desktop reached over a wire that carries media
    mountOnWire(true);
    hostDesktopPage.connect(HOST).should("exist");

    // When the only wire reaching that host is a frame pipe, which carries rpc and nothing else
    mountOnWire(false);

    // Then the action is absent rather than disabled — a video track genuinely cannot arrive over a
    // frame pipe, and offering a control that cannot work is worse than not offering one.
    hostDesktopPage.connect(HOST).should("not.exist");
  });

  it("opens the overlay for the selected host", () => {
    // Given a connectable host
    mountWithMedia();

    // When the operator connects
    hostDesktopPage.connect(HOST).click();

    // Then that host's desktop opens in the existing overlay
    hostDesktopPage.overlay(HOST).should("exist");
  });
});
