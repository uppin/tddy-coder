/**
 * Acceptance tests: the top-right daemon selector must stay live with the common room's actual
 * participant roster, driven through the *real* data path
 * (`SelectedDaemonProvider` -> `useRoomParticipants` -> `daemonHostsFromParticipants`) rather than
 * the static `daemons` injection seam the other tests use.
 *
 * Reported bug: the selector "doesn't pick up a new connected daemon and after page reload the
 * selector is just empty". Root cause: `useRoomParticipants` only re-syncs on per-participant
 * `ParticipantConnected` / `ParticipantDisconnected` / `ParticipantMetadataChanged` events. A
 * LiveKit auto-reconnect (network blip, LiveKit restart, dev-server churn, reload cycle) reuses the
 * same `Room` and re-delivers the existing peer roster via a single `RoomEvent.Reconnected` — not
 * per-peer connect events — so daemons present across a reconnect never (re)enter the list.
 *
 * These tests drive a `FakeCommonRoom` that emits those real `RoomEvent`s.
 *
 * PRD: docs/ft/web/daemon-selector-livekit-rpc.md.
 */

import React from "react";
import { DaemonSelector } from "../../src/components/shell/DaemonSelector";
import type { DaemonHost } from "../../src/lib/participantRole";
import { SelectedDaemonProvider, useSelectedDaemon } from "../../src/rpc/selectedDaemon";
import { daemonSelectorPage } from "../support/pages/daemonSelectorPage";
import { byTestId } from "../support/testIds";
import { aFakeCommonRoom } from "../support/livekit/fakeCommonRoom";

const UDOO: DaemonHost = { instanceId: "udoo", label: "udoo (this daemon)" };

/** Renders the connected selector against whatever the shared `SelectedDaemonProvider` derives. */
function DaemonSelectorReadout() {
  const { daemons, selectedInstanceId, servingInstanceId, selectDaemon } = useSelectedDaemon();
  return (
    <DaemonSelector
      daemons={daemons}
      selectedInstanceId={selectedInstanceId}
      servingInstanceId={servingInstanceId}
      onSelect={selectDaemon}
    />
  );
}

/**
 * Exposes the provider's *derived* daemon list directly. Radix `Select` caches the last-selected
 * item's rendered label on the trigger even after that item leaves the list, so the trigger cannot
 * distinguish "daemon still present" from "daemon gone but label cached" — this readout can.
 */
function DerivedDaemonIdsReadout() {
  const { daemons } = useSelectedDaemon();
  return (
    <span data-testid="derived-daemon-ids">
      {daemons.map((d) => d.instanceId).join(",") || "none"}
    </span>
  );
}

it("lists a daemon whose presence is re-synced when the common room reconnects", () => {
  // Given — connected to the common room with no daemons visible yet
  const commonRoom = aFakeCommonRoom();
  cy.mount(
    <SelectedDaemonProvider room={commonRoom.room} servingInstanceId="udoo">
      <DaemonSelectorReadout />
    </SelectedDaemonProvider>,
  );
  daemonSelectorPage.expectEmpty();

  // When — the daemon comes back as part of the roster re-synced on a LiveKit reconnect
  cy.then(() => commonRoom.reconnectWith([UDOO]));

  // Then — the selector reflects the reconnected daemon
  daemonSelectorPage.expectShowsSelected("udoo (this daemon)");
});

it("re-lists a daemon that dropped and rejoined across a common-room reconnect", () => {
  // Given — one daemon is connected and present in the selector's list
  const commonRoom = aFakeCommonRoom().withDaemons([UDOO]);
  cy.mount(
    <SelectedDaemonProvider room={commonRoom.room} servingInstanceId="udoo">
      <DaemonSelectorReadout />
      <DerivedDaemonIdsReadout />
    </SelectedDaemonProvider>,
  );
  byTestId("derived-daemon-ids").should("have.text", "udoo");

  // When — the daemon drops (list empties), then rejoins as part of the reconnect roster re-sync
  cy.then(() => commonRoom.disconnectDaemon("udoo"));
  byTestId("derived-daemon-ids").should("have.text", "none");
  cy.then(() => commonRoom.reconnectWith([UDOO]));

  // Then — the daemon is back in the list instead of the selector staying empty
  byTestId("derived-daemon-ids").should("have.text", "udoo");
});
