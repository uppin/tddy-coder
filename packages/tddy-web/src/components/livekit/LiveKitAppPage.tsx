import React from "react";
import { useSelectedDaemon } from "../../rpc/selectedDaemon";
import { useRoomParticipants } from "../../hooks/useRoomParticipants";
import { ParticipantList } from "../ParticipantList";
import { AppShell } from "../shell/AppShell";

/**
 * The LiveKit presence screen (`#/livekit`). Lists the participants in the shared common room
 * (browsers, daemons, coder sessions) — the "Connected participants" panel extracted from the old
 * ConnectionScreen.
 */
export function LiveKitAppPage({ onNavigate }: { onNavigate: (path: string) => void }) {
  // Take the connection state from the provider that owns the join, not from the room object: a
  // failed join leaves no room to observe, and reading `null` as "idle" is what made this panel
  // promise it was connecting to a room it had already given up on.
  const { room, roomStatus, roomError } = useSelectedDaemon();
  const participants = useRoomParticipants(room);

  return (
    <AppShell title="LiveKit" onNavigate={onNavigate} variant="scroll">
      <div
        data-testid="connected-participants-panel"
        className="rounded-md border border-border p-3"
      >
        <h3 className="mt-0 text-base font-semibold">Connected participants</h3>
        <ParticipantList
          participants={participants}
          roomStatus={roomStatus}
          connectionError={roomError}
        />
      </div>
    </AppShell>
  );
}
