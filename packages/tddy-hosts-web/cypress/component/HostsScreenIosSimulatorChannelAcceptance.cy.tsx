/**
 * Acceptance tests: a Mac's booted iOS Simulator is listed on its host row by device and runtime.
 *
 * PRD: docs/ft/web/1-WIP/PRD-2026-11-26-ios-simulator-screen-sharing.md (AC-IOS-1)
 */

import { create } from "@bufbuild/protobuf";
import { anInMemoryRpcBackend, type InMemoryRpcBackend } from "tddy-connectrpc-testkit";
import {
  DesktopChannelKind,
  HostRemoteDesktopSchema,
  HostService,
  ProbeOutcome,
} from "tddy-client-web/gen/host_pb";
import { Protocol, ScreenSharingService } from "tddy-client-web/gen/screen_sharing_pb";
import { aHostPromptFeed } from "../support/rpc/hostPromptFeed";
import { HostRowRemoteDesktop } from "../../src/HostRowRemoteDesktop";
import { hostDesktopChannelsPage } from "../support/pages/hostsScreenPage";

const HOST = "macbook-1";
const HOST_LISTED = [{ instanceId: HOST, label: HOST }];
const THE_SIMULATOR_TARGET = "host-target-iphone";

function aBootedIphone() {
  return create(HostRemoteDesktopSchema, {
    outcome: ProbeOutcome.OK,
    protocol: Protocol.IOS_SIMULATOR,
    canBridge: true,
    desktopReachable: true,
    port: 0,
    channelId: "ios:UDID-IPHONE",
    kind: DesktopChannelKind.IOS_SIMULATOR,
    label: "iPhone 16 Pro · iOS 18.2",
    deviceId: "UDID-IPHONE",
  });
}

/** A Mac that attaches the simulator and then holds its start open, as a bridge coming up does. */
function aMacStartingTheSimulator(): InMemoryRpcBackend {
  return anInMemoryRpcBackend()
    .implement(HostService, aHostPromptFeed().handlers)
    .implement(ScreenSharingService, {
      listHostTargets: async () => ({ targets: [] }),
      addHostTarget: async () => ({ targetId: THE_SIMULATOR_TARGET }),
      inspectHostDesktop: async () => ({ offered: [], unsupportedVncTypes: [], remembered: false }),
      startHostStream: () => new Promise(() => undefined),
      stopHostStream: async () => ({ ok: true }),
    });
}

function mountAMacWithABootedIphone(backend: InMemoryRpcBackend = anInMemoryRpcBackend()) {
  cy.mountWithClients(
    <HostRowRemoteDesktop instanceId={HOST} readings={[aBootedIphone()]} />,
    backend,
    { daemons: HOST_LISTED },
  );
}

describe("Hosts screen iOS Simulator channel", () => {
  it("names a booted simulator by its device and runtime", () => {
    // Given / When
    mountAMacWithABootedIphone();

    // Then
    hostDesktopChannelsPage
      .channel(HOST, "ios:UDID-IPHONE")
      .should("contain.text", "iPhone 16 Pro · iOS 18.2");
  });

  it("says whether the simulator's bridge is installed", () => {
    // Given / When
    mountAMacWithABootedIphone();

    // Then
    hostDesktopChannelsPage.channel(HOST, "ios:UDID-IPHONE").should("contain.text", "Bridge ready");
  });

  it("attaches the simulator by its device when its connect is pressed", () => {
    // Given
    const backend = aMacStartingTheSimulator();
    mountAMacWithABootedIphone(backend);

    // When
    hostDesktopChannelsPage.connectTo(HOST, "ios:UDID-IPHONE").click();

    // Then
    cy.wrap(backend).should((b: InMemoryRpcBackend) => {
      const [added] = b.callsTo(ScreenSharingService.method.addHostTarget);
      expect(added?.protocol, "attached as a simulator").to.equal(Protocol.IOS_SIMULATOR);
      expect(added?.deviceId, "naming the device pressed").to.equal("UDID-IPHONE");
      expect(added?.host, "on the Mac that runs it").to.equal("127.0.0.1");
      expect(added?.port, "with no port").to.equal(0);
      expect(added?.label, "called by its channel").to.equal("iPhone 16 Pro · iOS 18.2");
    });
  });

  it("starts the simulator's stream without inspecting it for a sign-in", () => {
    // Given
    const backend = aMacStartingTheSimulator();
    mountAMacWithABootedIphone(backend);

    // When
    hostDesktopChannelsPage.connectTo(HOST, "ios:UDID-IPHONE").click();

    // Then
    cy.wrap(backend).should((b: InMemoryRpcBackend) => {
      const [started] = b.callsTo(ScreenSharingService.method.startHostStream);
      expect(started?.targetId, "the stream starts on the simulator's target").to.equal(
        THE_SIMULATOR_TARGET,
      );
      expect(b.callsTo(ScreenSharingService.method.inspectHostDesktop)).to.have.length(0);
    });
  });
});
