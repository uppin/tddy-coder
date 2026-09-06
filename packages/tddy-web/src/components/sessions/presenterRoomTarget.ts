import type { SessionAttachmentHint } from "../../rpc/connections/session";

export interface PresenterRoomTarget {
  url: string;
  roomName: string;
  identity: string;
}

const defaultMakeIdentity = () => `browser-presenter-${Math.random().toString(36).slice(2, 10)}`;

/**
 * Derive the PR-Stack Chat Screen's own dedicated LiveKit room target from the attached session's
 * routing hint — the same room/url the session's own connection joins for its terminal
 * to. Returns `null` for a session that names no room: one the host serves itself has nothing for a
 * second participant to join, and neither has a session that is not attached at all.
 *
 * `makeIdentity` is injectable so callers (see `usePresenterLiveKitRoom`) can supply a stable,
 * per-room identity instead of a fresh one on every call — a distinct participant from the
 * terminal's own browser identity and from the observer identity the session connection joins under.
 */
export function presenterRoomTargetFor(
  hint: SessionAttachmentHint | null,
  makeIdentity: () => string = defaultMakeIdentity,
): PresenterRoomTarget | null {
  if (!hint?.room) return null;
  return {
    url: hint.url ?? "",
    roomName: hint.room,
    identity: makeIdentity(),
  };
}
