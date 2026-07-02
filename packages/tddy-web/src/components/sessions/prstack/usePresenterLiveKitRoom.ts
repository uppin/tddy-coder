import { useRef } from "react";
import type { Room } from "livekit-client";
import { useCommonRoom, type CommonRoomStatus } from "../../../hooks/useCommonRoom";
import type { SessionAttachmentState } from "../useSessionAttachment";
import { presenterRoomTargetFor } from "./presenterRoomTarget";

const IDENTITY_UNUSED = () => "";

/**
 * Connects the PR-Stack Chat Screen's own dedicated LiveKit room for the currently attached
 * session — independent of `SessionMainPane`'s always-null VNC-purpose `room` prop, and of the
 * terminal's own connection. Mirrors `useSessionLiveKitRoom` (used by `StatusBar` for the
 * traffic meter's own independent connection to the same room).
 */
export function usePresenterLiveKitRoom(
  attachment: SessionAttachmentState,
): { room: Room | null; status: CommonRoomStatus; error: string | null } {
  // presenterRoomTargetFor's own identity field is discarded here — it isn't stable across
  // renders on its own; this hook manages a stable identity ref instead (below), only
  // regenerated when the room name actually changes.
  const target = presenterRoomTargetFor(attachment, IDENTITY_UNUSED);

  const prevRoomRef = useRef<string | undefined>(undefined);
  const identityRef = useRef<string | undefined>(undefined);
  if (target?.roomName !== prevRoomRef.current) {
    prevRoomRef.current = target?.roomName;
    identityRef.current = target
      ? `browser-presenter-${Math.random().toString(36).slice(2, 10)}`
      : undefined;
  }

  const { room, status, error } = useCommonRoom(target?.url, target?.roomName, identityRef.current);
  return { room, status, error };
}
