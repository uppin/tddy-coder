import { useEffect, useState } from "react";
import { RoomEvent, type Participant, type Room } from "livekit-client";
import type { RoomHandle } from "tddy-client-web";
import { asRoom } from "../roomHandle";
import { hasRenderableCameraVideo } from "./participantCameraVideo";

/** The room member called `identity`: this browser's own participant, or a remote one. */
export function participantIn(room: Room, identity: string): Participant | undefined {
  if (room.localParticipant?.identity === identity) return room.localParticipant;
  return room.remoteParticipants?.get(identity);
}

const TRACK_EVENTS = [
  RoomEvent.ParticipantConnected,
  RoomEvent.ParticipantDisconnected,
  RoomEvent.TrackSubscribed,
  RoomEvent.TrackUnsubscribed,
  RoomEvent.TrackMuted,
  RoomEvent.TrackUnmuted,
  RoomEvent.LocalTrackPublished,
  RoomEvent.LocalTrackUnpublished,
] as const;

/** Whether `identity` has camera video worth previewing in `handle`; re-evaluates as tracks come and go. */
export function useHasCameraVideo(handle: RoomHandle | null, identity: string): boolean {
  const room = asRoom(handle);
  const read = () => {
    const participant = room ? participantIn(room, identity) : undefined;
    return participant !== undefined && hasRenderableCameraVideo(participant);
  };
  const [has, setHas] = useState(read);
  useEffect(() => {
    setHas(read());
    if (room === null || typeof room.on !== "function") return;
    const sync = () => setHas(read());
    for (const event of TRACK_EVENTS) room.on(event, sync);
    return () => {
      for (const event of TRACK_EVENTS) room.off(event, sync);
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [room, identity]);
  return has;
}
