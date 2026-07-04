/**
 * Acceptance test: the common-room LiveKit connection can drop and reconnect transiently (network
 * blip, LiveKit server restart, dev-server HMR) — observed live via `useCommonRoom`'s debug log
 * showing a `disconnect` followed by a reconnect a few seconds later, with the daemon list going
 * empty in between (`useRoomParticipants(null)` while the room is down).
 *
 * The selected daemon must survive that gap: a transient empty daemon list must not reset
 * `selectedInstanceId` to `null`, since (a) the daemon reappears moments later and the reset causes
 * a visible "nothing selected" flash even though a daemon actually is connected, and (b) resetting
 * to `null` makes every `useDaemonClient` consumer's RPC client `null` for the whole gap.
 *
 * PRD: docs/ft/web/daemon-selector-livekit-rpc.md.
 */

import React, { useState } from "react";
import { Room } from "livekit-client";
import { DaemonSelector } from "../../src/components/shell/DaemonSelector";
import type { DaemonHost } from "../../src/lib/participantRole";
import { SelectedDaemonProvider, useSelectedDaemon } from "../../src/rpc/selectedDaemon";
import { daemonSelectorPage } from "../support/pages/daemonSelectorPage";
import { byTestId } from "../support/testIds";

const DEV: DaemonHost = { instanceId: "dev", label: "dev (this daemon)" };

/**
 * Simulates the common room's daemon list flickering empty and back — exactly what
 * `useRoomParticipants` produces across a `useCommonRoom` disconnect/reconnect cycle, since
 * `SelectedDaemonProvider`'s `daemons` test-injection override stands in for that derived list.
 */
function RoomDisconnectHarness() {
  const [daemons, setDaemons] = useState<DaemonHost[]>([DEV]);
  return (
    <SelectedDaemonProvider room={new Room()} daemons={daemons} servingInstanceId="dev">
      <button data-testid="simulate-disconnect" onClick={() => setDaemons([])}>
        disconnect
      </button>
      <button data-testid="simulate-reconnect" onClick={() => setDaemons([DEV])}>
        reconnect
      </button>
      <SelectedDaemonReadout />
    </SelectedDaemonProvider>
  );
}

function SelectedDaemonReadout() {
  const { daemons, selectedInstanceId, servingInstanceId, selectDaemon } = useSelectedDaemon();
  return (
    <>
      <span data-testid="selected-instance-id">{selectedInstanceId ?? "none"}</span>
      <DaemonSelector
        daemons={daemons}
        selectedInstanceId={selectedInstanceId}
        servingInstanceId={servingInstanceId}
        onSelect={selectDaemon}
      />
    </>
  );
}

it("keeps the selected daemon through a transient room disconnect instead of resetting to none", () => {
  // Given — the serving daemon is selected while present
  cy.mount(<RoomDisconnectHarness />);
  byTestId("selected-instance-id").should("have.text", "dev");

  // When — the common room drops (daemon list goes empty, mirroring a real disconnect)
  byTestId("simulate-disconnect").click();

  // Then — the prior selection is preserved, not reset to "none"
  byTestId("selected-instance-id").should("have.text", "dev");

  // When — the room reconnects and the daemon reappears
  byTestId("simulate-reconnect").click();

  // Then — still correctly selected, and the selector reflects it
  byTestId("selected-instance-id").should("have.text", "dev");
  daemonSelectorPage.trigger().should("contain.text", "dev (this daemon)");
});
