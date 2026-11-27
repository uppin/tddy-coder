/**
 * Acceptance tests: the desktop's own host, reached over a wire that carries calls only, can show a
 * simulator's screen when this page also reaches that host over a common room with media.
 *
 * Calls stay on the host's own wire; the stream is watched in the room the start reply names.
 *
 * PRD: docs/ft/web/screen-sharing-sessions.md § iOS Simulator (AC-IOS-1)
 */

import { create } from "@bufbuild/protobuf";
import { anInMemoryRpcBackend, type InMemoryRpcBackend } from "tddy-connectrpc-testkit";
import type { SessionAttachmentHint } from "tddy-client-web/connections/session";
import type { HostConnection } from "tddy-client-web/connections/types";
import { ConnectionProviderRegistry, type RoomState } from "tddy-client-web";
import { aMediaDouble } from "tddy-client-web/testkit";
import {
  DesktopChannelKind,
  HostRemoteDesktopSchema,
  HostService,
  ProbeOutcome,
} from "tddy-client-web/gen/host_pb";
import { Protocol, ScreenSharingService } from "tddy-client-web/gen/screen_sharing_pb";
import { HostRowRemoteDesktop } from "../../src/HostRowRemoteDesktop";
import { HostRowToolingCell } from "../../src/HostRowToolingCell";
import { aHostPromptFeed } from "../support/rpc/hostPromptFeed";
import { aHostConnection } from "../support/rpc/hostConnections";
import { hostDesktopChannelsPage } from "../support/pages/hostsScreenPage";

const THIS_MAC = "this-mac";
const HOST_LISTED = [{ instanceId: THIS_MAC, label: THIS_MAC }];
const THE_COMMON_ROOM = "tddy-common";
const THE_MEDIA_SERVER = "ws://127.0.0.1:7880";

function aBootedIphone() {
  return create(HostRemoteDesktopSchema, {
    outcome: ProbeOutcome.OK,
    protocol: Protocol.IOS_SIMULATOR,
    canBridge: true,
    desktopReachable: true,
    channelId: "ios:UDID-IPHONE",
    kind: DesktopChannelKind.IOS_SIMULATOR,
    label: "iPhone 15 Pro · iOS 17.5",
    deviceId: "UDID-IPHONE",
  });
}

/**
 * This Mac's routing id when its daemon appends its startup timestamp: what it answers on in the
 * common room this run, while its row is named by the durable id.
 */
const THIS_MACS_ROUTING_ID = `${THIS_MAC}-1795723989119`;

/** This Mac's daemon, which attaches the simulator and starts it into the common room. */
function thisMacsDaemon(): InMemoryRpcBackend {
  return anInMemoryRpcBackend()
    .implement(HostService, {
      ...aHostPromptFeed().handlers,
      getHostTooling: async () => ({ remoteDesktop: [aBootedIphone()] }),
    })
    .implement(ScreenSharingService, {
      listHostTargets: async () => ({ targets: [] }),
      addHostTarget: async () => ({ targetId: "host-target-iphone" }),
      startHostStream: async () => ({
        livekitRoom: THE_COMMON_ROOM,
        livekitUrl: THE_MEDIA_SERVER,
        bridgeIdentity: "bridge-iphone",
        trackName: "screenshare:host-target-iphone",
        width: 1179,
        height: 2556,
      }),
      stopHostStream: async () => ({ ok: true }),
    });
}

/** The desktop's own wire to this Mac — calls only, as an IPC frame pipe carries. */
function theDesktopsOwnWireTo(backend: InMemoryRpcBackend, hostId = THIS_MAC): HostConnection {
  return aHostConnection(hostId).servingOver(backend.transport()).build();
}

/** A common room this page has joined, with this Mac's daemon on it. */
function aCommonRoomReachingThisMac(backend: InMemoryRpcBackend, hostId = THIS_MAC): HostConnection {
  return aHostConnection(hostId).reachedOverLiveKit().servingOver(backend.transport()).build();
}

/** The desktop's own wire first, then whatever else reaches this Mac — the desktop build's order. */
function wiresInPrecedenceOrder(...connections: HostConnection[]): ConnectionProviderRegistry {
  const registry = new ConnectionProviderRegistry();
  connections.forEach((connection, index) =>
    registry.register({
      id: `wire-${index}`,
      connectHost: (hostId) => (hostId === connection.hostId ? connection : null),
    }),
  );
  return registry;
}

/** A media port that records each room the overlay asks to join. */
function aMediaPortRecordingRooms() {
  const joined: SessionAttachmentHint[] = [];
  const NO_ROOM: RoomState = { room: null, status: "idle", error: null };
  const media = aMediaDouble({
    usePresenterRoom: (hint) => {
      if (hint) joined.push(hint);
      return NO_ROOM;
    },
  });
  return { media, joined };
}

function mountThisMac(registry: ConnectionProviderRegistry, backend: InMemoryRpcBackend, media = aMediaDouble()) {
  cy.mountWithClients(<HostRowRemoteDesktop instanceId={THIS_MAC} readings={[aBootedIphone()]} />, backend, {
    daemons: HOST_LISTED,
    registry,
    media,
  });
}

describe("the desktop's own host with a common room", () => {
  it("offers to connect a simulator when the common room reaches this host", () => {
    // Given
    const backend = thisMacsDaemon();
    mountThisMac(
      wiresInPrecedenceOrder(theDesktopsOwnWireTo(backend), aCommonRoomReachingThisMac(backend)),
      backend,
    );

    // Then
    hostDesktopChannelsPage.connectTo(THIS_MAC, "ios:UDID-IPHONE").should("exist");
  });

  it("watches the simulator in the room the start names", () => {
    // Given
    const backend = thisMacsDaemon();
    const { media, joined } = aMediaPortRecordingRooms();
    mountThisMac(
      wiresInPrecedenceOrder(theDesktopsOwnWireTo(backend), aCommonRoomReachingThisMac(backend)),
      backend,
      media,
    );

    // When
    hostDesktopChannelsPage.connectTo(THIS_MAC, "ios:UDID-IPHONE").click();

    // Then
    cy.wrap(joined).should((hints: SessionAttachmentHint[]) => {
      const last = hints[hints.length - 1];
      expect(last?.room, "the room the stream was started into").to.equal(THE_COMMON_ROOM);
      expect(last?.url, "on the media server the start named").to.equal(THE_MEDIA_SERVER);
    });
  });

  it("offers no connect when nothing but its own wire reaches this host", () => {
    // Given
    const backend = thisMacsDaemon();
    mountThisMac(wiresInPrecedenceOrder(theDesktopsOwnWireTo(backend)), backend);

    // Then
    hostDesktopChannelsPage.channel(THIS_MAC, "ios:UDID-IPHONE").should("exist");
    hostDesktopChannelsPage.connects(THIS_MAC).should("not.exist");
  });

  it("reaches this host by the id it answers on this run, not the id that names its row", () => {
    // Given both wires reach this Mac only by its suffixed routing id
    const backend = thisMacsDaemon();
    const registry = wiresInPrecedenceOrder(
      theDesktopsOwnWireTo(backend, THIS_MACS_ROUTING_ID),
      aCommonRoomReachingThisMac(backend, THIS_MACS_ROUTING_ID),
    );
    cy.mountWithClients(
      <HostRowToolingCell instanceId={THIS_MAC} addressInstanceId={THIS_MACS_ROUTING_ID} online />,
      backend,
      { daemons: HOST_LISTED, registry },
    );

    // When
    hostDesktopChannelsPage.connectTo(THIS_MAC, "ios:UDID-IPHONE").click();

    // Then
    cy.wrap(backend).should((b: InMemoryRpcBackend) => {
      const [added] = b.callsTo(ScreenSharingService.method.addHostTarget);
      expect(added?.daemonInstanceId, "the desktop is attached on the routing id").to.equal(
        THIS_MACS_ROUTING_ID,
      );
    });
  });
});
