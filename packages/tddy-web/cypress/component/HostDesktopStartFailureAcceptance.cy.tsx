/**
 * Acceptance tests: what the host desktop viewer does with what the start actually reported.
 *
 * The daemon now waits for its bridge to say whether the desktop came up (`#host-desktops` 6/8):
 * a refused sign-in fails `StartHostStream` as `Unauthenticated` carrying the desktop's reason, and
 * a started desktop reports the size its bridge saw. The viewer has to act on both — say the
 * desktop refused the sign-in rather than "Connecting…" for ever, and scale a click into the size
 * the host reported rather than an assumed 1920×1080.
 *
 * Feature doc: docs/ft/web/screen-sharing-sessions.md (AC-SS-5, AC-SS-11)
 */

import { create } from "@bufbuild/protobuf";
import { Code, ConnectError } from "@connectrpc/connect";
import { anInMemoryRpcBackend } from "tddy-connectrpc-testkit";
import { HostRemoteDesktopSchema, ProbeOutcome } from "../../src/gen/host_pb";
import { Protocol, ScreenSharingService } from "../../src/gen/screen_sharing_pb";
import { HostRowRemoteDesktop } from "../../src/components/hosts/HostRowRemoteDesktop";
import { SessionsDrawerScreen } from "../../src/components/sessions/SessionsDrawerScreen";
import { mountWithRpc } from "../support/rpc/inMemory";
import { mountWithRecordingLiveKitRpc } from "../support/rpc/recordingLiveKitRpc";
import { withSelectedDaemon } from "../support/rpc/withSelectedDaemon";
import { aBridgeRecordingInput, BUTTON } from "../support/rpc/screenSharingInputBridge";
import { desktopPage } from "../support/pages/screenSharingOverlayPage";
import { hostDesktopPage } from "../support/pages/hostsScreenPage";
import { sessionsDrawerPage } from "../support/pages/sessionsDrawerPage";
import { aSessionsDrawerBackend } from "../support/rpc/screenSharingBackend";

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

/** A session with a desktop saved on it — the session inspector's own way into a desktop. */
const A_SESSION = {
  sessionId: "ss-start-failure-0000-0000-0000-000000000001",
  createdAt: "2026-10-18T09:00:00Z",
  status: "active",
  repoPath: "/home/dev/start-failure-project",
  pid: 33333,
  isActive: false,
  projectId: "proj-ss-start-failure",
  daemonInstanceId: "",
  workflowGoal: "Start failure session",
  pendingElicitation: false,
};
const A_SESSION_TARGET = {
  id: "t-start-failure-001",
  label: "Build Box",
  host: "192.168.10.9",
  port: 5900,
  protocol: Protocol.VNC,
  username: "",
};
/** The session list is open by default only on a desktop-width window. */
const A_DESKTOP_WINDOW = { width: 1280, height: 800 };

/** A session whose saved desktop turns the sign-in down, failing the start as the daemon does. */
function aSessionWhoseDesktopRefusesTheSignIn() {
  return aSessionsDrawerBackend([A_SESSION])
    .onUnary(ScreenSharingService.method.listTargets, () => ({ targets: [A_SESSION_TARGET] }))
    .onUnary(ScreenSharingService.method.startStream, () => {
      throw new ConnectError(THE_DESKTOPS_REASON, Code.Unauthenticated);
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

  it("says on the session's target row that the desktop refused the sign in", () => {
    // Given a session whose saved desktop turns the password down, open on its Screen Sharing tab
    cy.viewport(A_DESKTOP_WINDOW.width, A_DESKTOP_WINDOW.height);
    window.localStorage.setItem("tddy_session_token", "fake-token");
    mountWithRpc(withSelectedDaemon(<SessionsDrawerScreen />), aSessionWhoseDesktopRefusesTheSignIn());
    sessionsDrawerPage.drawerItem(A_SESSION.sessionId).click();
    sessionsDrawerPage.inspectorScreenSharingTab().click();

    // When the operator starts that desktop
    sessionsDrawerPage.screenSharingStartBtn(A_SESSION_TARGET.id).click();

    // Then that target's row says the desktop refused the sign-in, in the desktop's own words
    sessionsDrawerPage
      .screenSharingStartError(A_SESSION_TARGET.id)
      .should("have.attr", "role", "alert")
      .and("have.text", `The desktop refused the sign-in: ${THE_DESKTOPS_REASON}`);
  });
});
