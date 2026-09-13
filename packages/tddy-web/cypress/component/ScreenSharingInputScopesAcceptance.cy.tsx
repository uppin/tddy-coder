/**
 * Acceptance tests: input forwarding works on both desktops tddy can open.
 *
 * PRD: docs/ft/web/1-WIP/PRD-2026-09-07-input-forwarding.md — AC-IF-4 (host-scoped) and AC-IF-5
 * (session-scoped, the pre-existing path, unregressed).
 *
 * The overlay's own contract — what is forwarded, how it is scaled, what the chord keeps — is
 * pinned once in `ScreenSharingInputForwardingAcceptance`. What only these two can show is that the
 * operator reaches that overlay from both entry points, and that the input goes to **this**
 * desktop's bridge: the two scopes address different participants, in different rooms, from
 * different start replies, and a client that hard-coded either would still pass a spec mounted on
 * the overlay alone.
 */

import React from "react";
import { create } from "@bufbuild/protobuf";
import { anInMemoryRpcBackend } from "tddy-connectrpc-testkit";
import { HostRemoteDesktopSchema, ProbeOutcome } from "../../src/gen/host_pb";
import { Protocol, ScreenSharingService } from "../../src/gen/screen_sharing_pb";
import { HostRowRemoteDesktop } from "../../src/components/hosts/HostRowRemoteDesktop";
import { SessionsDrawerScreen } from "../../src/components/sessions/SessionsDrawerScreen";
import { mountWithRecordingLiveKitRpc, type RecordedRpcCall } from "../support/rpc/recordingLiveKitRpc";
import { withSelectedDaemon } from "../support/rpc/withSelectedDaemon";
import { aSessionsDrawerBackend } from "../support/rpc/screenSharingBackend";
import { aBridgeRecordingInput, KEYSYM } from "../support/rpc/screenSharingInputBridge";
import { desktopPage } from "../support/pages/screenSharingOverlayPage";
import { hostDesktopPage } from "../support/pages/hostsScreenPage";
import { sessionsDrawerPage } from "../support/pages/sessionsDrawerPage";

// ---------------------------------------------------------------------------
// AC-IF-4 — a host's desktop
// ---------------------------------------------------------------------------

const HOST = "workstation-1";
const VNC = 1;
const A_HOST_TARGET = "target-host-desktop-1";
/** The participant `StartHostStream` names — where this host's input has to go, and nowhere else. */
const THE_HOSTS_BRIDGE = "bridge-workstation-1";

function aReachableVnc() {
  return create(HostRemoteDesktopSchema, {
    outcome: ProbeOutcome.OK,
    protocol: VNC,
    canBridge: true,
    desktopReachable: true,
    port: 5900,
  });
}

/** A host that opens its desktop straight away — this spec is not about the password prompt. */
function aHostWhoseDesktopOpens() {
  return anInMemoryRpcBackend().implement(ScreenSharingService, {
    listHostTargets: async () => ({ targets: [] }),
    addHostTarget: async () => ({ targetId: A_HOST_TARGET }),
    startHostStream: async () => ({
      livekitRoom: `host-desktop-${HOST}`,
      livekitUrl: "wss://livekit.example.test",
      bridgeIdentity: THE_HOSTS_BRIDGE,
      trackName: "desktop",
      width: 1920,
      height: 1080,
    }),
    stopHostStream: async () => ({ ok: true }),
  });
}

describe("Input on a host's desktop", () => {
  beforeEach(() => {
    cy.viewport(1200, 800);
    cy.clearLocalStorage();
  });

  it("types into the desktop opened from a host row", () => {
    // Given an operator who opened a reachable host's desktop from its row
    const bridge = aBridgeRecordingInput();
    const backend = bridge.servedBy(aHostWhoseDesktopOpens());
    const recorded = mountWithRecordingLiveKitRpc(
      withSelectedDaemon(<HostRowRemoteDesktop instanceId={HOST} readings={[aReachableVnc()]} />),
      backend,
    );
    hostDesktopPage.connect(HOST).click();
    hostDesktopPage.stream().should("exist");

    // When they type on it
    desktopPage.tapKey("Enter");

    // Then the host's bridge was told which key went down and came up
    bridge.expectKeyStrokes(
      { keysym: KEYSYM.Enter, pressed: true },
      { keysym: KEYSYM.Enter, pressed: false },
    );

    // …on a stream addressed at the participant this host's own start reply named. The two scopes
    // share one overlay and one in-memory backend, so without this a client that sent every
    // desktop's input to some other participant would look identical here.
    cy.wrap(recorded.rpcCalls).should((calls: RecordedRpcCall[]) => {
      const inputStreams = calls.filter((c) => c.method === "StreamInput");
      expect(inputStreams, "one input stream, opened against the host's bridge").to.have.length(1);
      expect(inputStreams[0].targetIdentity).to.equal(THE_HOSTS_BRIDGE);
    });
  });
});

// ---------------------------------------------------------------------------
// AC-IF-5 — a session's desktop, the path that already existed
// ---------------------------------------------------------------------------

const SESSION = {
  sessionId: "ss-input-session-1111-0000-0000-0000-000000000009",
  createdAt: "2026-09-07T11:00:00Z",
  status: "active",
  repoPath: "/home/dev/input-project",
  pid: 33333,
  isActive: false,
  projectId: "proj-ss-input",
  daemonInstanceId: "",
  workflowGoal: "Input forwarding session",
  pendingElicitation: false,
};

const A_SESSION_TARGET = {
  id: "t-input-001",
  label: "Dev Box",
  host: "192.168.10.5",
  port: 5900,
  protocol: Protocol.VNC,
  username: "",
};

/** The participant `StartStream` names for a session target. */
const THE_SESSIONS_BRIDGE = `screenshare-bridge-${A_SESSION_TARGET.id}`;

describe("Input on a session's desktop", () => {
  beforeEach(() => {
    cy.viewport(1280, 800);
    cy.clearLocalStorage();
    cy.clearAllSessionStorage();
    window.localStorage.setItem("tddy_session_token", "fake-token");
  });

  it("types into the desktop opened from a session's screen-sharing tab", () => {
    // Given an operator who started a session target's stream from the inspector
    const bridge = aBridgeRecordingInput();
    const backend = bridge.servedBy(
      aSessionsDrawerBackend([SESSION])
        .onUnary(ScreenSharingService.method.listTargets, () => ({ targets: [A_SESSION_TARGET] }))
        .onUnary(ScreenSharingService.method.startStream, () => ({
          livekitRoom: "room-ss-input",
          livekitUrl: "ws://127.0.0.1:7880",
          bridgeIdentity: THE_SESSIONS_BRIDGE,
          trackName: `screenshare:${A_SESSION_TARGET.id}`,
          width: 1920,
          height: 1080,
        }))
        .onUnary(ScreenSharingService.method.stopStream, () => ({ ok: true })),
    );
    // Mounted through the recording adapter, not `mountWithRpc`: that one hands every LiveKit
    // client the same in-memory transport and throws `targetIdentity` away, so a client that sent
    // this session's input to the host's bridge — or to a hard-coded participant — would look
    // exactly like a correct one here, and `THE_SESSIONS_BRIDGE` would be a fixture nothing reads.
    const recorded = mountWithRecordingLiveKitRpc(
      withSelectedDaemon(<SessionsDrawerScreen />),
      backend,
    );
    sessionsDrawerPage.drawerItem(SESSION.sessionId).click();
    sessionsDrawerPage.inspectorScreenSharingTab().click();
    sessionsDrawerPage.screenSharingStartBtn(A_SESSION_TARGET.id).click();
    desktopPage.root().should("exist");

    // When they type on it
    desktopPage.tapKey("Enter");

    // Then the session's bridge was told — the same overlay, reached the way it already was
    bridge.expectKeyStrokes(
      { keysym: KEYSYM.Enter, pressed: true },
      { keysym: KEYSYM.Enter, pressed: false },
    );

    // …on a stream addressed at the participant *this session target's* start reply named, which is
    // a different participant from the host scope's above. Both scopes share one overlay and one
    // in-memory backend, so this is the only assertion in either that can tell them apart.
    cy.wrap(recorded.rpcCalls).should((calls: RecordedRpcCall[]) => {
      const inputStreams = calls.filter((c) => c.method === "StreamInput");
      expect(inputStreams, "one input stream, opened against the session's bridge").to.have.length(1);
      expect(inputStreams[0].targetIdentity).to.equal(THE_SESSIONS_BRIDGE);
    });
  });
});
