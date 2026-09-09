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
 *
 * The two panels do not answer to the same wire, so the screen no longer gates itself as though
 * they did. The roster is presence and only presence — on a wire that carries none it says so, in
 * its own words, and that gate lives in `ParticipantList` where the roster is. The room list is
 * plain daemon RPC, and a host reached without LiveKit — the desktop over IPC — can still be asked
 * what rooms its LiveKit server holds and who is joined to each. Withholding the whole screen for
 * the roster's sake hid that from the one build most likely to need it.
 *
 * So the screen always renders, and each panel answers for itself. A bookmark or a shared link
 * lands on something real rather than on an explanation, and nothing here has to become "the real
 * screen" later — it already is.
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
  // The wire the roster arrives over: `ParticipantList` reads its `presence` and `media`
  // capabilities itself and reports on them, which is why this screen no longer resolves an
  // availability verdict of its own. `roomStatus` is the join it reports on, and stays this
  // screen's to pass down — the panel is told which room's status it is speaking for.
  const connection = useHostConnection(selectedInstanceId);
  const roomStatus = commonRoom?.status ?? "idle";

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
            roomStatus={roomStatus}
            connectionError={commonRoom?.error ?? null}
            connection={connection}
          />
        </div>
        <LiveKitRoomsPanel />
      </AppShell>
    </TooltipProvider>
  );
}
