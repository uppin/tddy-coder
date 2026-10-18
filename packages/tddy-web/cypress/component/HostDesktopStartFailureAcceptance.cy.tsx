/**
 * Acceptance tests: what the host desktop viewer does with what the start actually reported.
 *
 * The daemon now waits for its bridge to say whether the desktop came up (`#host-desktops` 6/8):
 * a refused sign-in fails `StartHostStream` as `Unauthenticated` carrying the desktop's reason, and
 * a started desktop reports the size its bridge saw. The viewer has to act on both — say the
 * desktop refused the sign-in rather than "Connecting…" for ever, and scale a click into the size
 * the host reported rather than an assumed 1920×1080.
 *
 * PRD: docs/ft/web/1-WIP/PRD-2026-10-18-host-desktops-bridge-ready.md
 */

import { create } from "@bufbuild/protobuf";
import { Code, ConnectError } from "@connectrpc/connect";
import { anInMemoryRpcBackend } from "tddy-connectrpc-testkit";
import { HostRemoteDesktopSchema, ProbeOutcome } from "../../src/gen/host_pb";
import { ScreenSharingService } from "../../src/gen/screen_sharing_pb";
import { HostRowRemoteDesktop } from "../../src/components/hosts/HostRowRemoteDesktop";
import { mountWithRpc } from "../support/rpc/inMemory";
import { mountWithRecordingLiveKitRpc } from "../support/rpc/recordingLiveKitRpc";
import { withSelectedDaemon } from "../support/rpc/withSelectedDaemon";
import { aBridgeRecordingInput, BUTTON } from "../support/rpc/screenSharingInputBridge";
import { desktopPage } from "../support/pages/screenSharingOverlayPage";
import { hostDesktopPage } from "../support/pages/hostsScreenPage";

const HOST = "workstation-1";
/** The directory lists the host its row is about — a row exists only for a listed host. */
const HOST_LISTED = [{ instanceId: HOST, label: HOST }];
const VNC = 1;
const A_HOST_TARGET = "target-host-desktop-1";
const THE_HOSTS_BRIDGE = "bridge-workstation-1";

/** What the desktop itself said when it turned the password down, as the daemon passes it on. */
const THE_DESKTOPS_REASON = "the password is not right for this desktop";

/** A desktop that is not full HD — the size its bridge saw, and the only size a click can land in. */
const THE_REPORTED_DESKTOP = { width: 1280, height: 800 };
/** Watched at half that size, so the reported size and an assumed 1920×1080 aim differently. */
const A_HALF_SIZE_WINDOW = { width: 640, height: 400 };
const THE_MIDDLE_OF_THE_WINDOW = { x: 320, y: 200 };
/** The middle of a 1280×800 desktop. An overlay scaling into 1920×1080 would send 960,540. */
const THE_MIDDLE_OF_THE_REPORTED_DESKTOP = { x: 640, y: 400 };

function aReachableVnc() {
  return create(HostRemoteDesktopSchema, {
    outcome: ProbeOutcome.OK,
    protocol: VNC,
    canBridge: true,
    desktopReachable: true,
    port: 5900,
  });
}

/** A host whose desktop turns the sign-in down: the start fails the way the daemon now fails it. */
function aHostWhoseDesktopRefusesTheSignIn() {
  return anInMemoryRpcBackend().implement(ScreenSharingService, {
    listHostTargets: async () => ({ targets: [] }),
    addHostTarget: async () => ({ targetId: A_HOST_TARGET }),
    startHostStream: async () => {
      throw new ConnectError(THE_DESKTOPS_REASON, Code.Unauthenticated);
    },
    stopHostStream: async () => ({ ok: true }),
  });
}

/** A host whose desktop opens, reporting the size its bridge saw. */
function aHostWhoseDesktopOpensAt(size: { width: number; height: number }) {
  return anInMemoryRpcBackend().implement(ScreenSharingService, {
    listHostTargets: async () => ({ targets: [] }),
    addHostTarget: async () => ({ targetId: A_HOST_TARGET }),
    startHostStream: async () => ({
      livekitRoom: `host-desktop-${HOST}`,
      livekitUrl: "wss://livekit.example.test",
      bridgeIdentity: THE_HOSTS_BRIDGE,
      trackName: "desktop",
      width: size.width,
      height: size.height,
    }),
    stopHostStream: async () => ({ ok: true }),
  });
}

function theHostRow() {
  return withSelectedDaemon(
    <HostRowRemoteDesktop instanceId={HOST} readings={[aReachableVnc()]} />,
    HOST_LISTED,
  );
}

describe("Host desktop start failure", () => {
  beforeEach(() => {
    cy.viewport(1200, 800);
    cy.clearLocalStorage();
  });

  it("says the desktop refused the sign in instead of waiting", () => {
    // Given a host whose desktop turns the password down
    mountWithRpc(theHostRow(), aHostWhoseDesktopRefusesTheSignIn());

    // When the operator opens it
    hostDesktopPage.connect(HOST).click();

    // Then the viewer says the desktop refused the sign-in, in the desktop's own words, and has
    // stopped saying it is connecting
    hostDesktopPage
      .status(HOST)
      .should("contain.text", "The desktop refused the sign-in")
      .and("contain.text", THE_DESKTOPS_REASON)
      .and("not.contain.text", "Connecting");
  });

  it("scales a click into the size the host reported", () => {
    // Given a 1280×800 desktop opened from its host row and watched at half size
    const bridge = aBridgeRecordingInput();
    mountWithRecordingLiveKitRpc(
      theHostRow(),
      bridge.servedBy(aHostWhoseDesktopOpensAt(THE_REPORTED_DESKTOP)),
    );
    hostDesktopPage.connect(HOST).click();
    hostDesktopPage.stream().should("exist");
    desktopPage.renderedAt(A_HALF_SIZE_WINDOW);

    // When the operator clicks in the middle of the picture
    desktopPage.pressLeftButtonAt(THE_MIDDLE_OF_THE_WINDOW);

    // Then the click lands in the middle of the desktop, in the desktop's own pixels
    bridge.expectPointerLandings({ ...THE_MIDDLE_OF_THE_REPORTED_DESKTOP, buttonMask: BUTTON.left });
  });
});
