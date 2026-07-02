import type { SessionAttachmentState } from "../useSessionAttachment";

export interface PresenterRoomTarget {
  url: string;
  roomName: string;
  identity: string;
}

const defaultMakeIdentity = () => `browser-presenter-${Math.random().toString(36).slice(2, 10)}`;

/**
 * Derive the PR-Stack Chat Screen's own dedicated LiveKit room target from the session's
 * `connectSession`/`resumeSession` attachment — the same room/url the terminal
 * (`SessionLiveKitTerminal`) independently connects to. Returns `null` until the attachment is
 * actually connected over LiveKit (idle, connecting, connected-grpc, and error all yield `null`).
 *
 * `makeIdentity` is injectable so callers (see `usePresenterLiveKitRoom`) can supply a stable,
 * per-room identity instead of a fresh one on every call — a distinct participant from the
 * terminal's own `attachment.identity`, matching the existing `web-traffic-*` pattern
 * `useSessionLiveKitRoom` uses for a second independent connection to the same room.
 */
export function presenterRoomTargetFor(
  attachment: SessionAttachmentState,
  makeIdentity: () => string = defaultMakeIdentity,
): PresenterRoomTarget | null {
  if (attachment.status !== "connected-livekit") return null;
  return {
    url: attachment.livekitUrl,
    roomName: attachment.livekitRoom,
    identity: makeIdentity(),
  };
}
