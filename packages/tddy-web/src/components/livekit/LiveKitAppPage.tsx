import React from "react";
import { useSelectedDaemon } from "../../rpc/selectedDaemon";
import { useRoomParticipants } from "../../hooks/useRoomParticipants";
import { useObservedCommonRoomStatus } from "../../hooks/useCommonRoom";
import { ParticipantList } from "../ParticipantList";
import { AppShell } from "../shell/AppShell";

/**
 * The LiveKit presence screen (`#/livekit`). Lists the participants in the shared common room
 * (browsers, daemons, coder sessions) — the "Connected participants" panel extracted from the old
 * ConnectionScreen.
 */
export function LiveKitAppPage({ onNavigate }: { onNavigate: (path: string) => void }) {
  const { room } = useSelectedDaemon();
  const participants = useRoomParticipants(room);
  const { status, error } = useObservedCommonRoomStatus(room);

  return (
    <AppShell title="LiveKit" onNavigate={onNavigate} variant="scroll">
      <div
        data-testid="connected-participants-panel"
        className="rounded-md border border-border p-3"
      >
        <h3 className="mt-0 text-base font-semibold">Connected participants</h3>
        <ParticipantList participants={participants} roomStatus={status} connectionError={error} />
      </div>
    </AppShell>
  );
}
