/**
 * Full-screen screen-sharing desktop overlay.
 *
 * Subscribes to the bridge participant's video track via the session's LiveKit room
 * and renders it as a full-screen overlay. Captures pointer and keyboard events.
 *
 * Dismiss via close button, Escape key, or clicking the backdrop.
 */

import React, { useEffect, useRef, useState } from "react";
import { RoomEvent, type Room, type VideoTrack } from "livekit-client";

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

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        e.preventDefault();
        onClose();
      }
    };
    document.addEventListener("keydown", onKey);
    return () => document.removeEventListener("keydown", onKey);
  }, [onClose]);

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
        <button
          data-testid="screen-sharing-overlay-close"
          type="button"
          className="absolute top-4 right-4 z-10 px-3 py-1 text-sm bg-black/60 text-white rounded hover:bg-black/80"
          onClick={onClose}
        >
          Close
        </button>
        <video
          data-testid="screen-sharing-overlay-video"
          ref={videoRef}
          className="max-w-full max-h-full"
          playsInline
          muted
        />
      </div>
    </div>
  );
}
