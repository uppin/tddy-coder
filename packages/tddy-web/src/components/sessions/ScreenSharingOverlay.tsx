/**
 * Full-screen screen-sharing desktop overlay.
 *
 * Subscribes to the bridge participant's video track via the session's LiveKit room
 * and renders it as a full-screen overlay. Captures the operator's pointer and keyboard and
 * forwards them to that bridge (`useScreenSharingInput`), so the desktop can be used and not only
 * watched.
 *
 * Dismiss via close button, the `Ctrl+Alt+Esc` chord, or clicking the backdrop. **Escape alone goes
 * to the remote desktop**, which is why the chord exists: an overlay that swallowed Escape left
 * every full-screen application on the far side unreachable.
 */

import React, { useEffect, useRef, useState } from "react";
import { RoomEvent, type Room, type VideoTrack } from "livekit-client";
import { useScreenSharingInput } from "./useScreenSharingInput";

export interface ScreenSharingOverlayProps {
  room: Room | null;
  bridgeIdentity: string;
  trackName: string;
  width: number;
  height: number;
  onClose: () => void;
}

export function ScreenSharingOverlay({
  room,
  bridgeIdentity,
  trackName,
  width,
  height,
  onClose,
}: ScreenSharingOverlayProps): React.ReactElement {
  const videoRef = useRef<HTMLVideoElement>(null);
  const [videoTrack, setVideoTrack] = useState<VideoTrack | null>(null);

  useEffect(() => {
    if (!room) return;

    const findTrack = () => {
      const participant = room.remoteParticipants.get(bridgeIdentity);
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
  }, [room, bridgeIdentity, trackName]);

  useEffect(() => {
    const el = videoRef.current;
    if (!videoTrack || !el) return;
    videoTrack.attach(el);
    return () => {
      videoTrack.detach(el);
    };
  }, [videoTrack]);

  const { inputUnavailable } = useScreenSharingInput({
    picture: videoRef,
    room,
    bridgeIdentity,
    framebufferWidth: width,
    framebufferHeight: height,
    onCloseChord: onClose,
  });

  return (
    <div
      data-testid="screen-sharing-overlay"
      className="fixed inset-0 z-50 flex items-center justify-center bg-black"
      onMouseDown={(e) => {
        if (e.target === e.currentTarget) onClose();
      }}
    >
      <div
        className="relative w-full h-full flex items-center justify-center"
        onMouseDown={(e) => e.stopPropagation()}
      >
        <div className="absolute top-4 right-4 z-10 flex items-center gap-2">
          {/* Every other key goes to the remote machine, so an operator who has not memorised the
              way out has only the mouse left. Say it, rather than expect it to be known. */}
          <span
            data-testid="screen-sharing-overlay-chord"
            className="px-2 py-1 text-xs bg-black/60 text-white rounded"
          >
            Ctrl+Alt+Esc to close
          </span>
          <button
            data-testid="screen-sharing-overlay-close"
            type="button"
            className="px-3 py-1 text-sm bg-black/60 text-white rounded hover:bg-black/80"
            onClick={onClose}
          >
            Close
          </button>
        </div>
        <video
          data-testid="screen-sharing-overlay-video"
          ref={videoRef}
          className="max-w-full max-h-full"
          playsInline
          muted
        />
        {inputUnavailable && (
          <div
            data-testid="screen-sharing-overlay-input-unavailable"
            role="status"
            className="absolute bottom-4 left-1/2 -translate-x-1/2 px-3 py-1 text-sm bg-black/70 text-white rounded"
          >
            This desktop is not accepting input — you can watch it, but not control it.
          </div>
        )}
      </div>
    </div>
  );
}
