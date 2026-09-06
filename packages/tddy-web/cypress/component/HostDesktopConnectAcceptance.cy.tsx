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
    // Given a host reached over a connection that carries media
    mountWithMedia();
    hostDesktopPage.connect(HOST).should("exist");

    // When the same reachable host is not reachable over any media-carrying connection
    mountWithRpc(
      withSelectedDaemon(<HostRowRemoteDesktop instanceId={HOST} readings={[aReachableVnc()]} />, [
        { instanceId: "some-other-host", label: "some-other-host" },
      ]),
      anInMemoryRpcBackend(),
    );

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
