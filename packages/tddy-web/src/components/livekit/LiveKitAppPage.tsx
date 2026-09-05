import React from "react";
import { useSelectedDaemon } from "../../rpc/selectedDaemon";
import { LIVEKIT_SOURCE_ID } from "../../rpc/hostDirectory/liveKitSource";
import { useHostDirectorySource } from "../../rpc/hostDirectory/useHostDirectory";
import { useHostPresence } from "../../rpc/hostDirectory/useHostPresence";
import { useHostConnection } from "../../rpc/connections/registry";
import { useRoomParticipants } from "../../hooks/useRoomParticipants";
import { ParticipantList } from "../ParticipantList";
import { AppShell } from "../shell/AppShell";
import { TooltipProvider } from "../ui/tooltip";
import { LiveKitRoomsPanel } from "./LiveKitRoomsPanel";

/**
 * The LiveKit presence screen (`#/livekit`). Lists the participants in the shared common room
 * (browsers, daemons, coder sessions) — the "Connected participants" panel extracted from the old
 * ConnectionScreen — and, below it, every room on the LiveKit server with who is joined to each.
 *
 * The rooms panel's metadata cards are Radix tooltips, so the screen carries their provider. The
 * delay is zero: the card is the readout, not a hint about a control.
 */
export function LiveKitAppPage({ onNavigate }: { onNavigate: (path: string) => void }) {
  // Take the connection state from the common room's own directory source, not from the room object
  // and not from the directory as a whole: a failed join leaves no room to observe, and reading
  // `null` as "idle" is what made this panel promise it was connecting to a room it had already
  // given up on. The merged directory would be just as misleading the other way — it stays
  // `connected` on the strength of a source that has nothing to do with this screen.
  const { selectedInstanceId } = useSelectedDaemon();
  const commonRoom = useHostDirectorySource(LIVEKIT_SOURCE_ID);
  const room = useHostPresence(selectedInstanceId);
  const participants = useRoomParticipants(room);
  // The wire the roster arrives over — the participant camera column is gated on its `media`
  // capability, since a camera track arrives the same way the roster does.
  const connection = useHostConnection(selectedInstanceId);

  return (
    <TooltipProvider delayDuration={0}>
      <AppShell title="LiveKit" onNavigate={onNavigate} variant="scroll">
        <div
          data-testid="connected-participants-panel"
          className="rounded-md border border-border p-3"
        >
          <h3 className="mt-0 text-base font-semibold">Connected participants</h3>
          <ParticipantList
            participants={participants}
            roomStatus={commonRoom?.status ?? "idle"}
            connectionError={commonRoom?.error ?? null}
            connection={connection}
          />
        </div>
        <LiveKitRoomsPanel />
      </AppShell>
    </TooltipProvider>
  );
}
