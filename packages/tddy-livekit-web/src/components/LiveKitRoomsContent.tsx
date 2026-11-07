import { TooltipProvider } from "tddy-ui-web";
import {
  LIVEKIT_SOURCE_ID,
  useHostConnection,
  useHostDirectorySource,
  useHostPresence,
  useRoomParticipants,
  useSelectedDaemon,
} from "tddy-client-web";
import { LiveKitRoomsPanel } from "./LiveKitRoomsPanel";
import { ParticipantList, type ParticipantVideoPreview } from "./ParticipantList";

/**
 * The body of the LiveKit presence screen (`#/livekit`): the participants in the shared common room
 * (browsers, daemons, coder sessions) and, below them, every room on the LiveKit server with who is
 * joined to each. The app supplies the shell around it.
 *
 * The rooms panel's metadata cards are Radix tooltips, so the content carries their provider. The
 * delay is zero: the card is the readout, not a hint about a control.
 *
 * The two panels do not answer to the same wire, so the content does not gate itself as though they
 * did. The roster is presence and only presence — on a wire that carries none it says so, in its own
 * words, and that gate lives in `ParticipantList` where the roster is. The room list is plain daemon
 * RPC, and a host reached without LiveKit — the desktop over IPC — can still be asked what rooms its
 * LiveKit server holds and who is joined to each. So the content always renders, and each panel
 * answers for itself.
 */
export function LiveKitRoomsContent({ VideoPreview }: { VideoPreview?: ParticipantVideoPreview }) {
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
  // capabilities itself and reports on them. `roomStatus` is the join it reports on, and stays this
  // content's to pass down — the panel is told which room's status it is speaking for.
  const connection = useHostConnection(selectedInstanceId);
  const roomStatus = commonRoom?.status ?? "idle";

  return (
    <TooltipProvider delayDuration={0}>
      <div data-testid="connected-participants-panel" className="rounded-md border border-border p-3">
        <h3 className="mt-0 text-base font-semibold">Connected participants</h3>
        <ParticipantList
          participants={participants}
          roomStatus={roomStatus}
          connectionError={commonRoom?.error ?? null}
          connection={connection}
          VideoPreview={VideoPreview}
        />
      </div>
      <LiveKitRoomsPanel />
    </TooltipProvider>
  );
}
