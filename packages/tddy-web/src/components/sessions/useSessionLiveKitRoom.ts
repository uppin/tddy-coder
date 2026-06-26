import { useRef } from "react";
import { useCommonRoom, type CommonRoomStatus } from "../../hooks/useCommonRoom";
import type { Room } from "livekit-client";
import type { SessionAttachmentState } from "./useSessionAttachment";

/**
 * Connects a LiveKit Room for the currently attached session and returns it for
 * use by `useLiveKitPing` and the LiveKit traffic meter subscription.
 *
 * Generates a unique observer identity per room that is stable across re-renders
 * but changes when the session switches to a different room.
 */
export function useSessionLiveKitRoom(
  attachment: SessionAttachmentState,
): { room: Room | null; status: CommonRoomStatus } {
  const livekitUrl = attachment.status === "connected-livekit" ? attachment.livekitUrl : undefined;
  const livekitRoom = attachment.status === "connected-livekit" ? attachment.livekitRoom : undefined;

  // Stable identity per room — regenerated only when the room name changes.
  const prevRoomRef = useRef<string | undefined>(undefined);
  const identityRef = useRef<string | undefined>(undefined);
  if (livekitRoom !== prevRoomRef.current) {
    prevRoomRef.current = livekitRoom;
    identityRef.current = livekitRoom
      ? `web-traffic-${Math.random().toString(36).slice(2, 10)}`
      : undefined;
  }

  const { room, status } = useCommonRoom(livekitUrl, livekitRoom, identityRef.current);
  return { room, status };
}
