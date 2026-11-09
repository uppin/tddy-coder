import { useEffect, useRef, useState, type Ref } from "react";
import { RoomEvent, type VideoTrack } from "livekit-client";
import type { RemoteVideoProps } from "tddy-client-web";
import { asRoom } from "../roomHandle";

/**
 * The video track `trackName` that `identity` publishes in `room`, attached to a `<video>` for as
 * long as it is subscribed. Renders the element whether or not the track has arrived, so a parent
 * can hold a ref to the picture from the first frame.
 */
export function RemoteVideo({ room: handle, identity, trackName, className, testId, videoRef }: RemoteVideoProps) {
  const room = asRoom(handle);
  const ownRef = useRef<HTMLVideoElement>(null);
  const [videoTrack, setVideoTrack] = useState<VideoTrack | null>(null);

  useEffect(() => {
    if (!room) return;

    const findTrack = () => {
      const participant = room.remoteParticipants.get(identity);
      if (!participant) return;
      for (const pub of participant.trackPublications.values()) {
        if (pub.trackName === trackName && pub.track?.kind === "video") {
          setVideoTrack(pub.track as VideoTrack);
          return;
        }
      }
    };

    findTrack();
    room.on(RoomEvent.ParticipantConnected, findTrack);
    room.on(RoomEvent.TrackSubscribed, findTrack);

    return () => {
      room.off(RoomEvent.ParticipantConnected, findTrack);
      room.off(RoomEvent.TrackSubscribed, findTrack);
      setVideoTrack(null);
    };
  }, [room, identity, trackName]);

  useEffect(() => {
    const el = ownRef.current;
    if (!videoTrack || !el) return;
    videoTrack.attach(el);
    return () => {
      videoTrack.detach(el);
    };
  }, [videoTrack]);

  const setRefs: Ref<HTMLVideoElement> = (el) => {
    ownRef.current = el;
    if (typeof videoRef === "function") videoRef(el);
    else if (videoRef) (videoRef as { current: HTMLVideoElement | null }).current = el;
  };

  return <video data-testid={testId} ref={setRefs} className={className} playsInline muted />;
}
