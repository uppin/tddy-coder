/**
 * Acceptance tests: a host with several desktops offers each one as a channel with its own Connect.
 *
 * A host's tooling reading lists its endpoint probes (VNC :5900, RDP :3389) plus one channel per X11
 * display its daemon can open. Each connectable channel gets a Connect, and pressing one opens that
 * channel — for an X11 display, by asking the host for a target naming the display, which is what
 * makes its daemon start x11vnc for it.
 *
 * Feature: docs/ft/web/hosts-screen-tooling.md, docs/ft/web/screen-sharing-sessions.md (AC-SS-17)
 */

import { create } from "@bufbuild/protobuf";
import { anInMemoryRpcBackend, type InMemoryRpcBackend } from "tddy-connectrpc-testkit";
import {
  DesktopChannelKind,
  HostRemoteDesktopSchema,
  HostService,
  ProbeOutcome,
} from "../../src/gen/host_pb";
import { Protocol, ScreenSharingService } from "../../src/gen/screen_sharing_pb";
import { HostRowRemoteDesktop } from "../../src/components/hosts/HostRowRemoteDesktop";
import { mountWithRpc } from "../support/rpc/inMemory";
import { withSelectedDaemon } from "../support/rpc/withSelectedDaemon";
import { aHostPromptFeed } from "../support/rpc/hostPromptFeed";
import { hostDesktopChannelsPage } from "../support/pages/hostsScreenPage";

const HOST = "workstation-1";
/** The directory lists the host its row is about — a row exists only for a listed host. */
const HOST_LISTED = [{ instanceId: HOST, label: HOST }];
const THE_X11_TARGET = "host-target-display-1";

/** The VNC server already listening on the host's :5900. */
function aVncServerOnItsDefaultPort() {
  return create(HostRemoteDesktopSchema, {
    outcome: ProbeOutcome.OK,
    protocol: Protocol.VNC,
    canBridge: true,
    desktopReachable: true,
    port: 5900,
    channelId: "vnc:5900",
    kind: DesktopChannelKind.ENDPOINT,
  });
}

/**
 * X display :1, which the host's daemon would serve through x11vnc — or, with `canBridge: false`,
 * one it cannot (no x11vnc on the host).
 */
function x11DisplayOne({ canBridge = true }: { canBridge?: boolean } = {}) {
  return create(HostRemoteDesktopSchema, {
    outcome: ProbeOutcome.OK,
    protocol: Protocol.VNC,
    canBridge,
    desktopReachable: true,
    port: 0,
    channelId: "x11:1",
    kind: DesktopChannelKind.X11_DISPLAY,
    display: 1,
    label: "Display :1",
  });
}

/** A host that takes the start and then waits on the desktop's password, as a real one does. */
function aHostAwaitingADesktopPassword(): InMemoryRpcBackend {
  return anInMemoryRpcBackend()
    .implement(HostService, aHostPromptFeed().handlers)
    .implement(ScreenSharingService, {
      listHostTargets: async () => ({ targets: [] }),
      addHostTarget: async () => ({ targetId: THE_X11_TARGET }),
      startHostStream: () => new Promise(() => undefined),
      stopHostStream: async () => ({ ok: true }),
    });
}

function mountAHostWithTwoChannels(backend: InMemoryRpcBackend = anInMemoryRpcBackend()) {
  mountWithRpc(
    withSelectedDaemon(
      <HostRowRemoteDesktop
        instanceId={HOST}
        readings={[aVncServerOnItsDefaultPort(), x11DisplayOne()]}
      />,
      HOST_LISTED,
    ),
    backend,
  );
}

function mountAHostWithOnlyAnUnbridgeableX11Display() {
  mountWithRpc(
    withSelectedDaemon(
      <HostRowRemoteDesktop instanceId={HOST} readings={[x11DisplayOne({ canBridge: false })]} />,
      HOST_LISTED,
    ),
    anInMemoryRpcBackend(),
  );
}

describe("Hosts screen desktop channels", () => {
  it("offers one connect per connectable channel", () => {
    // Given / When
    mountAHostWithTwoChannels();

    // Then
    hostDesktopChannelsPage.connects(HOST).should("have.length", 2);
  });

  it("opens the channel whose connect was pressed", () => {
    // Given
    const backend = aHostAwaitingADesktopPassword();
    mountAHostWithTwoChannels(backend);

    // When
    hostDesktopChannelsPage.connectTo(HOST, "x11:1").click();

    // Then
    cy.wrap(backend).should((b: InMemoryRpcBackend) => {
      const [added] = b.callsTo(ScreenSharingService.method.addHostTarget);
      expect(added?.x11Display, "the target names the display that was pressed").to.equal(1);

      const [started] = b.callsTo(ScreenSharingService.method.startHostStream);
      expect(started?.targetId, "the stream starts on that display's target").to.equal(
        THE_X11_TARGET,
      );
    });
  });

  it("names each x11 channel by its display", () => {
    // Given / When
    mountAHostWithTwoChannels();

    // Then
    hostDesktopChannelsPage.channel(HOST, "x11:1").should("contain.text", "Display :1");
  });

  it("offers no connect for an x11 channel that cannot be bridged", () => {
    // Given / When
    mountAHostWithOnlyAnUnbridgeableX11Display();

    // Then
    hostDesktopChannelsPage.connects(HOST).should("not.exist");
  });

  it("still lists an x11 channel that cannot be bridged", () => {
    // Given / When
    mountAHostWithOnlyAnUnbridgeableX11Display();

    // Then
    hostDesktopChannelsPage.channel(HOST, "x11:1").should("contain.text", "Display :1");
  });
});
