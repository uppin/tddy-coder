/**
 * A selected host is a choice, and the app never replaces it on its own. Reloading while the common
 * room is still joining — when only the daemon that served the page is listed — keeps the host the
 * URL names, and the selector shows it as waiting until the room lists it.
 *
 * The provider is driven through its production seams: `servingInstanceId` makes the serving
 * daemon listed from the first render (the serving source), and `daemons` stands in for what the
 * common room has listed so far. Without `servingInstanceId` — a build no daemon served — an empty
 * room listing leaves the directory empty. A second `cy.mount` is a reload: `sessionStorage` and the URL
 * survive it, component state does not.
 *
 * PRD: docs/ft/web/url-state-routing.md § Host in the URL.
 */

import React, { useState } from "react";
import { Room } from "livekit-client";
import { DaemonSelectorConnected } from "../../src/components/shell/DaemonSelector";
import type { DaemonHost } from "../../src/lib/participantRole";
import { SelectedDaemonProvider } from "../../src/rpc/selectedDaemon";
import { appLocationPage } from "../support/pages/appLocationPage";
import { daemonSelectorPage } from "../support/pages/daemonSelectorPage";
import { byTestId } from "../support/testIds";

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

const UDOO: DaemonHost = { instanceId: "udoo", label: "udoo (this daemon)" };
const LAPTOP_B: DaemonHost = { instanceId: "laptop-b", label: "Laptop B" };

/** The common room has not listed anyone yet — only the serving daemon is known. */
const ROOM_STILL_JOINING: DaemonHost[] = [];
const ROOM_LISTS_BOTH: DaemonHost[] = [UDOO, LAPTOP_B];

const ROOM_LISTS_LAPTOP_B = "room-lists-laptop-b";
const ROOM_DROPS_LAPTOP_B = "room-drops-laptop-b";
const ROOM_EMPTIES = "room-empties";

/**
 * The dashboard, served by `servingInstanceId` (or by no daemon), with a common room whose listing
 * the test moves along. The controls sit outside the provider so a host change, which remounts the
 * screens, leaves them alone.
 */
function Dashboard({
  roomListing,
  servingInstanceId,
}: {
  roomListing: DaemonHost[];
  servingInstanceId?: string;
}) {
  const [room] = useState(() => new Room());
  const [daemons, setDaemons] = useState(roomListing);
  return (
    <>
      <button data-testid={ROOM_LISTS_LAPTOP_B} onClick={() => setDaemons(ROOM_LISTS_BOTH)}>
        laptop-b joins
      </button>
      <button data-testid={ROOM_DROPS_LAPTOP_B} onClick={() => setDaemons([UDOO])}>
        laptop-b leaves
      </button>
      <button data-testid={ROOM_EMPTIES} onClick={() => setDaemons([])}>
        everyone leaves
      </button>
      <SelectedDaemonProvider room={room} daemons={daemons} servingInstanceId={servingInstanceId}>
        <DaemonSelectorConnected />
      </SelectedDaemonProvider>
    </>
  );
}

const dashboard = {
  /** Load (or reload) the page, served by `udoo`, while the common room has listed `roomListing`. */
  loadWhileRoomLists(roomListing: DaemonHost[]) {
    cy.mount(<Dashboard roomListing={roomListing} servingInstanceId="udoo" />);
  },
  /** Load the page with no serving daemon, so only the common room lists hosts. */
  loadUnservedWhileRoomLists(roomListing: DaemonHost[]) {
    cy.mount(<Dashboard roomListing={roomListing} />);
  },
  laptopBJoinsTheRoom() {
    byTestId(ROOM_LISTS_LAPTOP_B).click();
  },
  laptopBLeavesTheRoom() {
    byTestId(ROOM_DROPS_LAPTOP_B).click();
  },
  everyoneLeavesTheRoom() {
    byTestId(ROOM_EMPTIES).click();
  },
};

// ---------------------------------------------------------------------------
// Setup
// ---------------------------------------------------------------------------

beforeEach(() => {
  cy.clearLocalStorage();
  cy.clearAllSessionStorage();
  appLocationPage.reset();
});

// ---------------------------------------------------------------------------
// Reload
// ---------------------------------------------------------------------------

it("keeps the URL's host after a reload while the common room has not listed it yet", () => {
  // Given
  appLocationPage.startAt("/sessions?host=laptop-b");

  // When
  dashboard.loadWhileRoomLists(ROOM_STILL_JOINING);

  // Then — once the selection has settled, the URL still names laptop-b
  daemonSelectorPage.expectShowsSelected("laptop-b");
  appLocationPage.expectParam("host", "laptop-b");
});

it("waits under the instance id for a host this tab has never listed", () => {
  // Given — a shared link opened in a fresh tab
  appLocationPage.startAt("/sessions?host=laptop-b");

  // When
  dashboard.loadWhileRoomLists(ROOM_STILL_JOINING);

  // Then
  daemonSelectorPage.expectWaitingFor("laptop-b");
});

it("shows the waiting host under the label it was last listed with", () => {
  // Given — this tab has seen laptop-b listed as "Laptop B"
  appLocationPage.startAt("/sessions?host=laptop-b");
  dashboard.loadWhileRoomLists(ROOM_LISTS_BOTH);
  daemonSelectorPage.expectShowsSelected("Laptop B");

  // When — the page is reloaded and the room has not listed it again yet
  dashboard.loadWhileRoomLists(ROOM_STILL_JOINING);

  // Then
  daemonSelectorPage.expectWaitingFor("Laptop B");
});

it("keeps a deep-linked session's address intact while its host is waiting", () => {
  // Given
  appLocationPage.startAt("/sessions/sess-on-laptop-b?host=laptop-b");

  // When
  dashboard.loadWhileRoomLists(ROOM_STILL_JOINING);

  // Then — once the selection has settled
  daemonSelectorPage.expectShowsSelected("laptop-b");
  appLocationPage.expectPath("/sessions/sess-on-laptop-b");
  appLocationPage.expectParam("host", "laptop-b");
});

it("stops waiting, without touching the URL, once the room lists the host", () => {
  // Given — reloaded onto laptop-b before the room has listed it
  appLocationPage.startAt("/sessions?host=laptop-b");
  dashboard.loadWhileRoomLists(ROOM_STILL_JOINING);
  daemonSelectorPage.expectWaitingFor("laptop-b");

  // When
  dashboard.laptopBJoinsTheRoom();

  // Then
  daemonSelectorPage.expectShowsSelectedExactly("Laptop B");
  daemonSelectorPage.expectNotWaiting();
  appLocationPage.expectParam("host", "laptop-b");
});

// ---------------------------------------------------------------------------
// A host that leaves
// ---------------------------------------------------------------------------

it("keeps a host that leaves the room selected and waits for it", () => {
  // Given — working on laptop-b
  appLocationPage.startAt("/sessions?host=laptop-b");
  dashboard.loadWhileRoomLists(ROOM_LISTS_BOTH);
  daemonSelectorPage.expectShowsSelected("Laptop B");

  // When
  dashboard.laptopBLeavesTheRoom();

  // Then
  daemonSelectorPage.expectWaitingFor("Laptop B");
  appLocationPage.expectParam("host", "laptop-b");
});

// ---------------------------------------------------------------------------
// The operator's own navigation
// ---------------------------------------------------------------------------

it("moves to another host when the operator chooses one while waiting", () => {
  // Given — waiting for laptop-b
  appLocationPage.startAt("/sessions?host=laptop-b");
  dashboard.loadWhileRoomLists(ROOM_STILL_JOINING);
  daemonSelectorPage.expectWaitingFor("laptop-b");

  // When
  daemonSelectorPage.choose("udoo");

  // Then
  appLocationPage.expectParam("host", "udoo");
  daemonSelectorPage.expectNotWaiting();
  daemonSelectorPage.expectShowsSelectedExactly("udoo (this daemon)");
});

it("waits for a host reached by Back instead of redirecting away from it", () => {
  // Given — the operator went from laptop-b to udoo, and laptop-b has since left the room
  appLocationPage.startAt("/sessions?host=laptop-b");
  dashboard.loadWhileRoomLists(ROOM_LISTS_BOTH);
  daemonSelectorPage.choose("udoo");
  appLocationPage.expectParam("host", "udoo");
  dashboard.laptopBLeavesTheRoom();

  // When
  appLocationPage.goBack();

  // Then
  appLocationPage.expectParam("host", "laptop-b");
  daemonSelectorPage.expectWaitingFor("Laptop B");
});

it("follows Back to another host while the directory lists no host at all", () => {
  // Given — no daemon served the page; the operator went from laptop-b to udoo, then the room emptied
  appLocationPage.startAt("/sessions?host=laptop-b");
  dashboard.loadUnservedWhileRoomLists(ROOM_LISTS_BOTH);
  daemonSelectorPage.choose("udoo");
  appLocationPage.expectParam("host", "udoo");
  dashboard.everyoneLeavesTheRoom();
  daemonSelectorPage.expectWaitingFor("udoo");

  // When
  appLocationPage.goBack();

  // Then — the selection follows the URL without waiting for the directory
  appLocationPage.expectParam("host", "laptop-b");
  daemonSelectorPage.expectWaitingFor("Laptop B");
});
