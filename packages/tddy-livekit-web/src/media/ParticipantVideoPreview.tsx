import { Track, type VideoTrack } from "livekit-client";
import type { ParticipantVideoPreviewProps } from "tddy-client-web";
import { asRoom } from "../roomHandle";
import { ParticipantVideoPreviewDialog } from "./ParticipantVideoPreviewDialog";
import { participantIn } from "./useHasCameraVideo";

function cameraTrackOf(room: ReturnType<typeof asRoom>, identity: string): VideoTrack | null {
  const participant = room ? participantIn(room, identity) : undefined;
  const publication = participant?.getTrackPublication(Track.Source.Camera);
  return (publication?.videoTrack as VideoTrack | undefined) ?? null;
}

/** The camera preview the roster opens for `identity`: the dialog, given that participant's camera track from `room`. */
export function ParticipantVideoPreview({ room, identity, open, onOpenChange }: ParticipantVideoPreviewProps) {
  return (
    <ParticipantVideoPreviewDialog
      identity={identity}
      open={open}
      onOpenChange={onOpenChange}
      videoTrack={open ? cameraTrackOf(asRoom(room), identity) : null}
    />
  );
}
