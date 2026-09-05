/**
 * Acceptance tests: the session inspector's media tabs exist only when the host serving the session
 * can carry video tracks.
 *
 * The host is the upstream fact. A session's own capabilities are derived from whether its attach
 * hint names a room (`capabilitiesForHint`), and whether there is a room at all is decided by how
 * the host is reached — a host reached without LiveKit can never hand out a room-backed session, so
 * it can never serve a VNC or screen-sharing track either. The two tabs are then *absent* rather
 * than dead: a tab the operator cannot use invites a support question with no good answer.
 *
 * Reading the *session's* connection instead would refuse the tabs for a dormant session, which has
 * no connection at all — pinned below by "offers the media tabs for a dormant session on a host
 * reached over LiveKit".
 *
 * Availability is read through `capabilityAvailability`, the same rule the presence surfaces use:
 * the common room's status first, the capability second. While a join is in flight there is no host
 * connection yet, so the capability alone would render seven tabs on load and nine a second later.
 * A host that never joins a room reports `idle` and is refused, which is every "reached without
 * LiveKit" scenario below — none of them puts a common room in the tree.
 *
 * Every absence here is asserted next to something that does render, because a `not.exist` on its
 * own is satisfied just as well by a component that threw.
 *
 * PRD: docs/dev/1-WIP/2026-09-05-optional-livekit-capability-gating-prd.md (AC 2, AC 4).
 * Changeset: docs/dev/1-WIP/2026-09-05-optional-livekit-capability-gating.md.
 */

import React from "react";
import { anInMemoryRpcBackend } from "tddy-connectrpc-testkit";
import { SessionMainPane } from "../../src/components/sessions/SessionMainPane";
import type { SessionAttachmentState } from "../../src/components/sessions/useSessionAttachment";
import type { SessionRuntimeState } from "../../src/components/sessions/sessionRuntimeRegistry";
import type { SessionEntry } from "../../src/gen/connection_pb";
import { ScreenSharingService } from "../../src/gen/screen_sharing_pb";
import { VncService } from "../../src/gen/vnc_pb";
import type { ConnectionStatus, HostConnection } from "../../src/rpc/connections/types";
import { LIVEKIT_SOURCE_ID } from "../../src/rpc/hostDirectory/liveKitSource";
import { HostDirectorySources } from "../../src/rpc/hostDirectory/useHostDirectory";
import type { HostDirectorySource } from "../../src/rpc/hostDirectory/types";
import { appLocationPage } from "../support/pages/appLocationPage";
import { sessionsDrawerPage as page } from "../support/pages/sessionsDrawerPage";
import { aHostConnection } from "../support/rpc/hostConnections";
import { mountWithRpc } from "../support/rpc/inMemory";
import { aSessionConnection } from "../support/rpc/sessionConnections";

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

const HOST_ID = "instance-media-capability";
const SESSION_ID = "media-capability-session-0000-0000-000000000001";

const SESSION: Partial<SessionEntry> = {
  sessionId: SESSION_ID,
  createdAt: "2026-09-05T09:00:00Z",
  status: "active",
  repoPath: "/home/dev/capability-project",
  pid: 4242,
  isActive: true,
  projectId: "proj-capability-gating",
  workflowGoal: "Capability-gated inspector",
};

/** A host on a common room — the wire a bridge can publish a video track over. */
function aHostReachedOverLiveKit(): HostConnection {
  return aHostConnection(HOST_ID)
    .reachedOverLiveKit()
    .servingOver(anInMemoryRpcBackend().transport())
    .build();
}

/** A host reached over a frame pipe — calls arrive, tracks cannot. */
function aHostReachedWithoutLiveKit(): HostConnection {
  return aHostConnection(HOST_ID).servingOver(anInMemoryRpcBackend().transport()).build();
}

/** The session is attached and its terminal is mounted — the ordinary case. */
function anAttachedRuntime(): SessionRuntimeState[] {
  return [
    {
      sessionId: SESSION_ID,
      attached: true,
      connection: aSessionConnection(SESSION_ID)
        .servingOver(anInMemoryRpcBackend().transport())
        .build(),
      bytesIn: 0,
      bytesOut: 0,
      lastDataReceivedAt: null,
    },
  ];
}

/**
 * The common room's own directory source, as the drawer reads it (`useHostDirectorySource`).
 *
 * A scenario that supplies none is a tree with no common room in it at all, which reads as `idle` —
 * the desktop build over IPC, and every "reached without LiveKit" case below.
 */
function aCommonRoomThatIs(status: ConnectionStatus): HostDirectorySource {
  return { id: LIVEKIT_SOURCE_ID, status, error: null, hosts: [] };
}

/**
 * What the registry answers while the common room is still being joined: `LiveKitConnections` is
 * bound to a `null` room until `Room.connect()` resolves, so no provider can reach the host yet.
 */
function noHostReachableYet(): null {
  return null;
}

/** A session with no process behind it: nothing was attached, so there is no session connection. */
function noRuntime(): SessionRuntimeState[] {
  return [];
}

/** The two listings a media panel fires on mount, so one that does render has somewhere to read. */
function aBackendServingBothMediaTabs() {
  return anInMemoryRpcBackend()
    .onUnary(VncService.method.listVncTargets, () => ({ targets: [] }))
    .onUnary(ScreenSharingService.method.listTargets, () => ({ targets: [] }));
}

function mountInspectorOn(
  host: HostConnection | null,
  runtimes: SessionRuntimeState[],
  commonRoom?: HostDirectorySource,
) {
  const attachment: SessionAttachmentState =
    runtimes.length > 0 && runtimes[0].connection
      ? { status: "connected", connection: runtimes[0].connection }
      : { status: "idle" };

  const inspector = (
    <SessionMainPane
      selectedSession={SESSION as SessionEntry}
      attachment={attachment}
      host={host}
      inspectorState="open"
      onToggleInspector={() => undefined}
      onInspectorClose={() => undefined}
      onInspectorExpand={() => undefined}
      onInspectorRestore={() => undefined}
      onResume={() => undefined}
      onDelete={() => undefined}
      onTerminate={() => undefined}
      runtimes={runtimes}
      focusedRuntimeId={null}
    />
  );

  mountWithRpc(
    commonRoom ? (
      <HostDirectorySources sources={[commonRoom]}>{inspector}</HostDirectorySources>
    ) : (
      inspector
    ),
    aBackendServingBothMediaTabs(),
  );
}

// ---------------------------------------------------------------------------
// Setup
// ---------------------------------------------------------------------------

beforeEach(() => {
  cy.viewport(1280, 800);
  appLocationPage.reset();
});

// ---------------------------------------------------------------------------
// The tab strip
// ---------------------------------------------------------------------------

it("keeps the VNC tab out of the strip on a host reached without LiveKit", () => {
  // Given a host whose wire carries no tracks
  // When the inspector opens on one of its sessions
  mountInspectorOn(aHostReachedWithoutLiveKit(), anAttachedRuntime());

  // Then the strip is there, and VNC is not one of the tabs on it
  page.inspectorDetailsTab().should("exist");
  page.inspectorVncTab().should("not.exist");
});

it("offers the VNC tab on a host reached over LiveKit", () => {
  // Given a host on a common room
  // When the inspector opens on one of its sessions
  mountInspectorOn(aHostReachedOverLiveKit(), anAttachedRuntime());

  // Then VNC is offered exactly as it always was
  page.inspectorVncTab().should("exist");
});

it("keeps the Screen Sharing tab out of the strip on a host reached without LiveKit", () => {
  // Given a host whose wire carries no tracks
  // When the inspector opens on one of its sessions
  mountInspectorOn(aHostReachedWithoutLiveKit(), anAttachedRuntime());

  // Then the strip is there, and Screen Sharing is not one of the tabs on it
  page.inspectorDetailsTab().should("exist");
  page.inspectorScreenSharingTab().should("not.exist");
});

it("offers the Screen Sharing tab on a host reached over LiveKit", () => {
  // Given a host on a common room
  // When the inspector opens on one of its sessions
  mountInspectorOn(aHostReachedOverLiveKit(), anAttachedRuntime());

  // Then Screen Sharing is offered exactly as it always was
  page.inspectorScreenSharingTab().should("exist");
});

it("leaves every non-media tab in the strip on a host reached without LiveKit", () => {
  // Given a host whose wire carries no tracks
  // When the inspector opens on one of its sessions
  mountInspectorOn(aHostReachedWithoutLiveKit(), anAttachedRuntime());

  // Then only the two media tabs went — gating removes what the wire cannot serve, nothing else
  page.inspectorDetailsTab().should("exist");
  page.inspectorToolsTab().should("exist");
  page.inspectorAgentsTab().should("exist");
  page.inspectorUsageTab().should("exist");
  page.inspectorWorktreeTab().should("exist");
  page.inspectorFilesTab().should("exist");
});

it("keeps the media tabs in the strip while the common room is still being joined", () => {
  // Given the first second of a LiveKit page load: the join has not settled, so no provider can
  // reach the host yet and no connection carries `media`
  // When the inspector opens
  mountInspectorOn(noHostReachableYet(), anAttachedRuntime(), aCommonRoomThatIs("connecting"));

  // Then both media tabs are already there. Waiting on the capability alone would render seven tabs
  // and then nine, reflowing the strip under the operator's cursor a second after it settled
  page.inspectorDetailsTab().should("exist");
  page.inspectorVncTab().should("exist");
  page.inspectorScreenSharingTab().should("exist");
});

it("keeps the media tabs out of the strip on a host that joins no room at all", () => {
  // Given the desktop build over IPC: nothing is being joined, so the common room reports `idle`
  // rather than `connecting`, and it will not start later
  // When the inspector opens
  mountInspectorOn(aHostReachedWithoutLiveKit(), anAttachedRuntime(), aCommonRoomThatIs("idle"));

  // Then the tabs are absent and stay absent — an `idle` room is a decision, not a wait
  page.inspectorDetailsTab().should("exist");
  page.inspectorVncTab().should("not.exist");
  page.inspectorScreenSharingTab().should("not.exist");
});

it("offers the media tabs for a dormant session on a host reached over LiveKit", () => {
  // Given a session with no process behind it, and so no session connection to ask
  // When the inspector opens on it, on a host that can carry tracks
  mountInspectorOn(aHostReachedOverLiveKit(), noRuntime());

  // Then the tabs are there: the capability is the host's, and a session that was never attached
  // has not withdrawn it — target configuration is exactly what this operator came for
  page.inspectorVncTab().should("exist");
  page.inspectorScreenSharingTab().should("exist");
});

// ---------------------------------------------------------------------------
// A link that names a media tab
// ---------------------------------------------------------------------------

it("lands an ?inspector=vnc link on Details on a host reached without LiveKit", () => {
  // Given a link naming the VNC tab — pasted, or carried over from a host that had video
  appLocationPage.startAt(`/sessions/${SESSION_ID}?inspector=vnc`);

  // When it opens on a host whose wire carries no tracks
  mountInspectorOn(aHostReachedWithoutLiveKit(), anAttachedRuntime());

  // Then the drawer shows Details rather than a panel that could only ever be blank
  page.inspectorDetailsTab().should("have.attr", "aria-selected", "true");
  page.inspectorMetadata().should("exist");
  page.vncTabPanel().should("not.exist");
});

it("opens the VNC panel from an ?inspector=vnc link on a host reached over LiveKit", () => {
  // Given the same link
  appLocationPage.startAt(`/sessions/${SESSION_ID}?inspector=vnc`);

  // When it opens on a host on a common room
  mountInspectorOn(aHostReachedOverLiveKit(), anAttachedRuntime());

  // Then it lands on the VNC panel, as the link asked
  page.vncTabPanel().should("exist");
});

it("lands an ?inspector=screen-sharing link on Details on a host reached without LiveKit", () => {
  // Given a link naming the Screen Sharing tab
  appLocationPage.startAt(`/sessions/${SESSION_ID}?inspector=screen-sharing`);

  // When it opens on a host whose wire carries no tracks
  mountInspectorOn(aHostReachedWithoutLiveKit(), anAttachedRuntime());

  // Then the drawer shows Details rather than an overlay with nothing to subscribe to
  page.inspectorDetailsTab().should("have.attr", "aria-selected", "true");
  page.inspectorMetadata().should("exist");
  page.screenSharingTabPanel().should("not.exist");
});

it("opens the Screen Sharing panel from an ?inspector=screen-sharing link on a host reached over LiveKit", () => {
  // Given the same link
  appLocationPage.startAt(`/sessions/${SESSION_ID}?inspector=screen-sharing`);

  // When it opens on a host on a common room
  mountInspectorOn(aHostReachedOverLiveKit(), anAttachedRuntime());

  // Then it lands on the Screen Sharing panel, as the link asked
  page.screenSharingTabPanel().should("exist");
});
