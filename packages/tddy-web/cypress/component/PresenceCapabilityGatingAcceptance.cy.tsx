/**
 * Acceptance tests: every presence surface is absent, and says why, on a connection that carries no
 * LiveKit roster — and unchanged on one that does.
 *
 * Presence is not a detail of the LiveKit screen. The participant roster, the rooms panel, the
 * playground's participant picker and the sessions drawer's cross-host rows are all built on
 * common-room participants, so a host reached over a wire with no roster has none of them. Left
 * ungated they do not fail loudly: the roster claims it is "Connecting…" forever, and the drawer
 * quietly drops every row it cannot see, which reads as "this host has no other sessions".
 *
 * Two rules are pinned throughout. **Hidden, not disabled** — the LiveKit nav entry is removed
 * rather than greyed out, because an entry leading to an empty screen invites a support question
 * with no good answer; where an entry point has to stay (a `#/livekit` deep link, the drawer's own
 * list) it names the connection as the reason instead. And **one predicate** — nothing re-derives
 * "is there a roster here" from a transport or from the presence of a `Room`.
 *
 * Every absence is asserted next to something that does render, or next to a call that was or was
 * not made: a bare `not.exist` is satisfied just as well by a component that threw.
 *
 * Technical: packages/tddy-web/docs/capability-gating.md.
 */

import React from "react";
import type { InMemoryRpcBackend } from "tddy-connectrpc-testkit";
import { LiveKitAppPage } from "../../src/components/livekit/LiveKitAppPage";
import { ParticipantList } from "../../src/components/ParticipantList";
import { SessionsDrawerScreen } from "../../src/components/sessions/SessionsDrawerScreen";
import { DaemonNavMenu } from "../../src/components/shell/DaemonNavMenu";
import { RpcPlaygroundAppPage } from "../../src/rpc-playground/RpcPlaygroundAppPage";
import type { RoomParticipant } from "../../src/hooks/useRoomParticipants";
import { AuthProvider } from "../../src/hooks/authProvider";
import { ConnectionProviders } from "../../src/rpc/connections/registry";
import type { HostConnection } from "../../src/rpc/connections/types";
import type { CapabilityBearing } from "../../src/rpc/connections/useHasCapability";
import { SelectedDaemonProvider } from "../../src/rpc/selectedDaemon";
import { appLocationPage } from "../support/pages/appLocationPage";
import { appShellPage as shell } from "../support/pages/appShellPage";
import { liveKitRoomsPanelPage as roomsPanel } from "../support/pages/liveKitRoomsPanelPage";
import { liveKitScreenPage as liveKitScreen } from "../support/pages/liveKitScreenPage";
import { participantListPage as roster } from "../support/pages/participantListPage";
import { rpcPlaygroundPage as playground } from "../support/pages/rpcPlaygroundPage";
import { sessionsDrawerPage as drawer } from "../support/pages/sessionsDrawerPage";
import { aCommonRoomThatNeverFinishesConnecting } from "../support/livekit/commonRoomConnection";
import { aConnectionServiceBackend } from "../support/rpc/connectionServiceBackend";
import { aHostConnection, aRegistryServing } from "../support/rpc/hostConnections";
import { mountWithRpc } from "../support/rpc/inMemory";
import { mountWithLiveCommonRoom } from "../support/rpc/withLiveCommonRoom";
import {
  aLiveKitRoomsBackend,
  aRoom,
  type LiveKitRoomsBackend,
} from "../support/rpc/liveKitRoomsBackend";

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

const HOST_ID = "instance-presence-capability";
const HOST_LABEL = "workstation (this daemon)";
const A_ROOM_ON_THE_SERVER = "daemon-presenter-room-0001";

const A_SESSION_ON_THIS_HOST = {
  sessionId: "presence-gating-0000-4000-8000-000000000001",
  createdAt: "2026-09-05T09:00:00Z",
  status: "active",
  repoPath: "/home/dev/presence-project",
  pid: 5150,
  isActive: true,
  projectId: "proj-presence-gating",
  daemonInstanceId: HOST_ID,
  pendingElicitation: false,
};

/** A host reached over the common room: calls, tracks and a participant roster all ride the wire. */
function aHostReachedOverLiveKit(backend: InMemoryRpcBackend): HostConnection {
  return aHostConnection(HOST_ID).reachedOverLiveKit().servingOver(backend.transport()).build();
}

/** A host reached over a frame pipe: the calls arrive, and there is no roster to be had. */
function aHostReachedWithoutLiveKit(backend: InMemoryRpcBackend): HostConnection {
  return aHostConnection(HOST_ID).servingOver(backend.transport()).build();
}

/** The same two answers as props, for the presentational roster panel. */
function aWireThatCarriesARoster(): CapabilityBearing {
  return { capabilities: new Set(["rpc", "media", "presence"] as const) };
}

function aWireWithNoRoster(): CapabilityBearing {
  return { capabilities: new Set(["rpc"] as const) };
}

function aRosterOf(...identities: string[]): RoomParticipant[] {
  return identities.map((identity) => ({
    identity,
    role: "browser" as const,
    joinedAt: 1_700_000_000_000,
    metadata: "",
    codexOAuth: null,
  }));
}

/**
 * Mount `screen` on `host`, and on nothing else.
 *
 * `room={null}` is what makes the scenario say what it means: with no common room in the tree the
 * LiveKit provider claims no host and constructs no `livekit-client` `Room`, so the only wire that
 * reaches `HOST_ID` is the one the builder described. A `withSelectedDaemon` mount could not state
 * this at all — handing the provider a `Room` makes every host LiveKit-reached by construction.
 */
function mountOn(host: HostConnection, backend: InMemoryRpcBackend, screen: React.ReactElement) {
  window.localStorage.setItem("tddy_session_token", "fake-token");
  return mountWithRpc(
    <AuthProvider>
      <ConnectionProviders registry={aRegistryServing(host)}>
        <SelectedDaemonProvider
          room={null}
          daemons={[{ instanceId: HOST_ID, label: HOST_LABEL }]}
          servingInstanceId={HOST_ID}
        >
          {screen}
        </SelectedDaemonProvider>
      </ConnectionProviders>
    </AuthProvider>,
    backend,
  );
}

/** A daemon serving one room on its LiveKit server, and counting who asks for the feed. */
function aDaemonServingOneRoom(): LiveKitRoomsBackend {
  return aLiveKitRoomsBackend({ rooms: [aRoom({ name: A_ROOM_ON_THE_SERVER })] });
}

/** A daemon reporting one session of its own — what `ListSessions` alone can see. */
function aDaemonServingOneSession(): InMemoryRpcBackend {
  return aConnectionServiceBackend({ sessions: [A_SESSION_ON_THIS_HOST] });
}

beforeEach(() => {
  cy.viewport(1280, 800);
  cy.clearLocalStorage();
  cy.clearAllSessionStorage();
  appLocationPage.reset();
});

// ---------------------------------------------------------------------------
// The participant roster
// ---------------------------------------------------------------------------

it("names the connection as the reason there is no participant roster", () => {
  // Given a roster read over a wire that carries no presence
  // When the panel renders
  cy.mount(
    <ParticipantList
      participants={aRosterOf("browser-alice")}
      roomStatus="idle"
      connectionError={null}
      connection={aWireWithNoRoster()}
    />,
  );

  // Then it says so, and stops claiming it is connecting to a room it will never reach
  roster
    .unavailable()
    .should("contain.text", "not available on this connection")
    .and("contain.text", "carries no LiveKit presence");
  roster.list().should("have.attr", "data-room-status", "unavailable");
  roster.entry("browser-alice").should("not.exist");
});

it("lists the roster unchanged over a wire that carries presence", () => {
  // Given the same roster on a common room
  // When the panel renders
  cy.mount(
    <ParticipantList
      participants={aRosterOf("browser-alice")}
      roomStatus="connected"
      connectionError={null}
      connection={aWireThatCarriesARoster()}
    />,
  );

  // Then nothing about it changes — the whole node turns on this staying true
  roster.entry("browser-alice").should("contain.text", "browser-alice");
  roster.unavailable().should("not.exist");
});

it("keeps saying the join is in flight rather than blaming a connection that is still being made", () => {
  // Given a common room mid-join: `LiveKitConnections` is bound to a null room until `connect()`
  // resolves, so there is no connection to read a capability off yet
  // When the panel renders
  cy.mount(
    <ParticipantList
      participants={[]}
      roomStatus="connecting"
      connectionError={null}
      connection={null}
    />,
  );

  // Then it reports the join, and makes no claim about the wire that it would withdraw a second
  // later once the room is up
  roster.list().should("have.attr", "data-room-status", "connecting");
  roster.unavailable().should("not.exist");
});

// ---------------------------------------------------------------------------
// The LiveKit screen
// ---------------------------------------------------------------------------

it("explains the LiveKit screen instead of rendering it dead when reached by link", () => {
  // Given a host reached without LiveKit
  const rooms = aDaemonServingOneRoom();

  // When `#/livekit` is opened anyway — a bookmark, or a link from a host that did have presence
  mountOn(aHostReachedWithoutLiveKit(rooms.backend), rooms.backend, <LiveKitAppPage onNavigate={cy.stub()} />);

  // Then the screen names the connection as the reason it has nothing to show, and neither panel is
  // there to sit empty behind it
  liveKitScreen
    .unavailable()
    .should("contain.text", "not available on this connection")
    .and("contain.text", "carries no LiveKit presence");
  liveKitScreen.participantsPanel().should("not.exist");
  roomsPanel.panel().should("not.exist");
});

it("renders the roster and the room list on a host reached over the common room", () => {
  // Given the same host, reached over LiveKit
  const rooms = aDaemonServingOneRoom();

  // When the screen is opened
  mountOn(aHostReachedOverLiveKit(rooms.backend), rooms.backend, <LiveKitAppPage onNavigate={cy.stub()} />);

  // Then both panels are there, exactly as before this node
  liveKitScreen.participantsPanel().should("be.visible");
  roomsPanel.room(A_ROOM_ON_THE_SERVER).should("exist");
  liveKitScreen.unavailable().should("not.exist");
});

it("asks the daemon for no rooms feed at all when the connection has no presence", () => {
  // Given a host reached without LiveKit
  const rooms = aDaemonServingOneRoom();

  // When the LiveKit screen is opened on it
  mountOn(aHostReachedWithoutLiveKit(rooms.backend), rooms.backend, <LiveKitAppPage onNavigate={cy.stub()} />);

  // Then the panel is not merely hidden: the stream behind it is never subscribed, so the daemon is
  // not left serving a feed for a panel nobody can see
  liveKitScreen.unavailable().should("be.visible");
  cy.wrap(rooms).should((feed) => {
    expect(feed.roomsStreamCount()).to.equal(0);
  });
});

it("keeps the rooms panel in place, unsubscribed, while the common room is still being joined", () => {
  // Given a common room whose join has not settled: the panel's capability is absent for the second
  // or two every LiveKit page load spends here
  // When the screen renders
  mountWithLiveCommonRoom(<LiveKitAppPage onNavigate={cy.stub()} />, aCommonRoomThatNeverFinishesConnecting());

  // Then the panel holds its place and says what it is waiting on, rather than being absent and
  // dropping in underneath the roster once the join lands — the layout must not move under the
  // operator (PRD AC 7)
  roomsPanel.panel().should("be.visible");
  roomsPanel.joining().should("contain.text", "Joining the common room");

  // And the feed behind it is still not opened: the panel body and the subscription are two
  // different questions, and only the roster-carrying capability answers the second one. `Loading
  // rooms…` is what `RoomsFeed` renders before its first snapshot, so its absence is the mount's
  // absence.
  roomsPanel.loading().should("not.exist");
  roomsPanel.error().should("not.exist");
});

// ---------------------------------------------------------------------------
// The navigation entry
// ---------------------------------------------------------------------------

it("removes the LiveKit entry from the navigation menu on a connection with no presence", () => {
  // Given a host reached without LiveKit
  const backend = aConnectionServiceBackend();
  mountOn(aHostReachedWithoutLiveKit(backend), backend, <DaemonNavMenu onNavigate={cy.stub()} />);

  // When the operator opens the menu
  shell.openMenu();

  // Then the entry is gone rather than present-and-dead, and every other entry is untouched — the
  // full list is asserted, so a removal that took a neighbour with it fails here
  shell
    .menuItemLabels()
    .should("deep.equal", [
      "Sessions",
      "Worktrees",
      "Tasks",
      "Projects",
      "Models & Agents",
      "VMs",
      "RPC Playground",
      "Settings",
    ]);
  shell.livekitItem().should("not.exist");
});

it("keeps the LiveKit entry while the common room is still being joined", () => {
  // Given a common room whose join has not settled: there is no connection to read a capability off
  // yet, exactly as on the first second of every LiveKit page load
  // When the operator opens the menu
  mountWithLiveCommonRoom(<DaemonNavMenu onNavigate={cy.stub()} />, aCommonRoomThatNeverFinishesConnecting());
  shell.openMenu();

  // Then the entry is still there. Hiding it here would take it away and put it back on every load,
  // and the screen it leads to has a join to report on — the entry and the screen read the same rule
  shell.livekitItem().should("be.visible");
});

it("offers the LiveKit entry on a host reached over the common room", () => {
  // Given a host reached over LiveKit
  const backend = aConnectionServiceBackend();
  const onNavigate = cy.stub().as("onNavigate");
  mountOn(aHostReachedOverLiveKit(backend), backend, <DaemonNavMenu onNavigate={onNavigate} />);

  // When the operator opens the menu and chooses LiveKit
  shell.openMenu();
  shell.livekitItem().click();

  // Then it routes to the screen exactly as it always did
  cy.get("@onNavigate").should("have.been.calledOnceWith", "/livekit");
});

// ---------------------------------------------------------------------------
// The RPC Playground's participant picker
// ---------------------------------------------------------------------------

it("replaces the playground's participant picker with the reason there is nobody to address", () => {
  // Given a host reached without LiveKit — the participants a playground call is addressed to are
  // common-room coder participants, and the call itself rides a LiveKit data channel
  const backend = aConnectionServiceBackend();

  // When the playground is opened
  mountOn(aHostReachedWithoutLiveKit(backend), backend, <RpcPlaygroundAppPage onNavigate={cy.stub()} />);

  // Then the rest of the screen is there and the picker is not a permanently empty select
  playground.serviceTree().should("exist");
  playground
    .participantSelectionUnavailable()
    .should("contain.text", "not available on this connection");
  playground.participantSelect().should("not.exist");
});

it("offers the playground's participant picker on a host reached over the common room", () => {
  // Given a host reached over LiveKit
  const backend = aConnectionServiceBackend();

  // When the playground is opened
  mountOn(aHostReachedOverLiveKit(backend), backend, <RpcPlaygroundAppPage onNavigate={cy.stub()} />);

  // Then the picker is offered, unchanged
  playground.participantSelect().should("exist");
  playground.participantSelectionUnavailable().should("not.exist");
});

it("keeps the playground's participant picker while the common room is still being joined", () => {
  // Given a join that has not settled — the picker's capability reads `false` here, though a roster
  // is seconds away
  // When the playground renders
  mountWithLiveCommonRoom(<RpcPlaygroundAppPage onNavigate={cy.stub()} />, aCommonRoomThatNeverFinishesConnecting());

  // Then it offers the select rather than announcing an absence it would take back once the room is
  // joined — the same rule the nav entry, the screen and the drawer read
  playground.participantSelect().should("exist");
  playground.participantSelectionUnavailable().should("not.exist");
});

// ---------------------------------------------------------------------------
// The sessions drawer's cross-host reconciliation
// ---------------------------------------------------------------------------

it("shows what ListSessions reports and says the rest is out of view when presence is absent", () => {
  // Given a host reached without LiveKit, reporting one session of its own. The cross-host rows the
  // drawer would normally union in are live common-room coder participants, and there are none to
  // observe on this wire.
  const backend = aDaemonServingOneSession();

  // When the drawer is opened
  mountOn(aHostReachedWithoutLiveKit(backend), backend, <SessionsDrawerScreen onNavigate={cy.stub()} />);

  // Then it shows the rows it does have, and says which ones it cannot see — rather than presenting
  // a narrowed list as the whole truth
  drawer.drawerItem(A_SESSION_ON_THIS_HOST.sessionId).should("exist");
  drawer
    .crossHostUnavailable()
    .should("contain.text", "Sessions on other hosts are not visible from this connection");
});

it("says nothing about other hosts while the common room is still being joined", () => {
  // Given a join that has not settled — no connection carries presence yet, though one is about to
  // When the drawer renders
  mountWithLiveCommonRoom(<SessionsDrawerScreen onNavigate={cy.stub()} />, aCommonRoomThatNeverFinishesConnecting());

  // Then the screen is up and makes no claim it would withdraw a second later, once the room is
  // joined and the cross-host rows arrive
  drawer.drawer().should("exist");
  drawer.crossHostUnavailable().should("not.exist");
});

it("claims nothing about other hosts when the connection can see them", () => {
  // Given the same host reached over the common room, where the cross-host union applies
  const backend = aDaemonServingOneSession();

  // When the drawer is opened
  mountOn(aHostReachedOverLiveKit(backend), backend, <SessionsDrawerScreen onNavigate={cy.stub()} />);

  // Then the list is the answer on its own — no footnote about rows it could not reach
  drawer.drawerItem(A_SESSION_ON_THIS_HOST.sessionId).should("exist");
  drawer.crossHostUnavailable().should("not.exist");
});
