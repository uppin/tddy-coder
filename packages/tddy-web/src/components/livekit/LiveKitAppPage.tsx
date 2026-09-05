import React from "react";
import { useSelectedDaemon } from "../../rpc/selectedDaemon";
import { LIVEKIT_SOURCE_ID } from "../../rpc/hostDirectory/liveKitSource";
import { useHostDirectorySource } from "../../rpc/hostDirectory/useHostDirectory";
import { useHostPresence } from "../../rpc/hostDirectory/useHostPresence";
import { useHostConnection } from "../../rpc/connections/registry";
import { useHasCapability } from "../../rpc/connections/useHasCapability";
import { useRoomParticipants } from "../../hooks/useRoomParticipants";
import { presenceAvailability } from "../../hooks/presenceAvailability";
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
 * Everything on this screen is presence. On a host reached over a wire that carries none there is
 * no roster and no room list, so the screen renders neither — it says why instead. The route stays
 * reachable and the URL is kept: the nav entry is gone (see `DaemonNavMenu`), but a bookmark, a
 * shared link, or a URL carried over from a host that did have presence must land somewhere that
 * explains itself, and must become the real screen the moment the wire can serve it. That is the
 * same treatment `SessionInspectorDrawer` gives a media tab named on a host with no tracks.
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
  // The wire the roster arrives over. Its `presence` capability decides whether this screen has
  // anything to show at all; its `media` capability decides the participant camera column, since a
  // camera track arrives the same way the roster does.
  const connection = useHostConnection(selectedInstanceId);
  const carriesPresence = useHasCapability(connection, "presence");
  const roomStatus = commonRoom?.status ?? "idle";
  const availability = presenceAvailability(roomStatus, carriesPresence);

  // Only when nothing is being joined and the wire has no presence either. A join still in flight,
  // or one that failed with a reason, is a roster that exists and is reported on by the panels
  // below — announcing "not available on this connection" for those would be a claim about the
  // wire that the next second contradicts.
  if (availability === "unavailable") {
    return (
      <AppShell title="LiveKit" onNavigate={onNavigate} variant="scroll">
        <div
          data-testid="livekit-unavailable"
          className="rounded-md border border-border p-3 text-sm text-muted-foreground"
        >
          LiveKit is not available on this connection: this host is reached over a wire that
          carries no LiveKit presence, so there is no participant roster and no room list to show.
        </div>
      </AppShell>
    );
  }

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
