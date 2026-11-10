import { useRef } from "react";
import type { RoomState } from "tddy-client-web";
import type { SessionAttachmentHint } from "tddy-client-web/connections/session";
import { useCommonRoom } from "../rooms/useCommonRoom";
import { asHandle } from "../roomHandle";
import { presenterRoomTargetFor } from "./presenterRoomTarget";

const IDENTITY_UNUSED = () => "";

/**
 * Connects the PR-Stack Chat Screen's own dedicated LiveKit room for the currently attached
 * session — independent of `SessionMainPane`'s always-null VNC-purpose `room` prop, and of the
 * terminal's own connection. A session whose hint names no room has nothing to join, and the hook
 * yields no room rather than a failed one.
 */
export function usePresenterRoom(hint: SessionAttachmentHint | null): RoomState {
  // presenterRoomTargetFor's own identity field is discarded here — it isn't stable across
  // renders on its own; this hook manages a stable identity ref instead (below), only
  // regenerated when the room name actually changes.
  const target = presenterRoomTargetFor(hint, IDENTITY_UNUSED);

  const prevRoomRef = useRef<string | undefined>(undefined);
  const identityRef = useRef<string | undefined>(undefined);
  if (target?.roomName !== prevRoomRef.current) {
    prevRoomRef.current = target?.roomName;
    identityRef.current = target
      ? `browser-presenter-${Math.random().toString(36).slice(2, 10)}`
      : undefined;
  }

  const { room, status, error } = useCommonRoom(target?.url, target?.roomName, identityRef.current);
  return { room: asHandle(room), status, error };
}
