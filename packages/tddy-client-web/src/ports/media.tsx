/**
 * Presence and media ports: rooms, their participants, and the media a room carries (camera video,
 * screen sharing, the presenter room). LiveKit is the only implementation (`tddy-livekit-web`), but
 * no component package sees a LiveKit type — a room is an opaque {@link RoomHandle}, and the port
 * supplies both the hooks that read it and the components that render its media.
 *
 * The ports carry hooks. Their identities must be stable for the app's lifetime (the app builds the
 * port once), so calling them obeys the rules of hooks like any imported hook.
 */
import type { ComponentType, ReactElement, ReactNode, RefObject } from "react";
import type { SessionAttachmentHint } from "../connections/session";

/** A joined room, opaque outside the implementation. */
export interface RoomHandle {
  readonly __room: unique symbol;
}

export type RoomStatus = "idle" | "connecting" | "connected" | "error";

/** A room the caller is joining or has joined, with how far it got. */
export interface RoomState {
  readonly room: RoomHandle | null;
  readonly status: RoomStatus;
  readonly error: string | null;
}

export type ParticipantRole = "browser" | "daemon" | "presenter" | "unknown" | (string & {});

export interface RoomParticipant {
  identity: string;
  role: ParticipantRole;
  joinedAt: number | null;
  metadata: string;
  /** Structured Codex OAuth hint from metadata, when present. */
  codexOAuth: { authorizeUrl: string; callbackPort: number } | null;
  /** From `owned_project_count` in metadata. */
  ownedProjectCount?: number | null;
}

export interface PresencePort {
  /** The presence room that answers for `hostId` (its common room), or `null`. */
  useHostPresence(hostId: string | null): RoomHandle | null;
  /** The presence room of the surrounding host scope, or `null`. */
  usePresenceRoom(): RoomHandle | null;
  /** Everyone in `room`, re-rendering on join, leave and metadata change. */
  useRoomParticipants(room: RoomHandle | null): RoomParticipant[];
}

export interface ScreenSharingInputOptions {
  readonly picture: RefObject<HTMLElement | null>;
  readonly room: RoomHandle | null;
  readonly bridgeIdentity: string;
  readonly framebufferWidth: number;
  readonly framebufferHeight: number;
  readonly onCloseChord: () => void;
}

export interface MediaPort {
  /** Join the presenter room a session's attachment hint names. */
  usePresenterRoom(hint: SessionAttachmentHint | null): RoomState;
  /** Forward pointer and keyboard input on `picture` to a screen-sharing bridge in `room`. */
  useScreenSharingInput(options: ScreenSharingInputOptions): { readonly inputUnavailable: boolean };
  /** Whether `participant` has camera video worth offering a preview of. */
  useHasCameraVideo(room: RoomHandle | null, identity: string): boolean;
  /** Renders the video track `trackName` of `identity` in `room`. */
  readonly RemoteVideo: ComponentType<{ room: RoomHandle | null; identity: string; trackName: string; className?: string }>;
}

export function PresencePortProvider(props: { port: PresencePort; children?: ReactNode }): ReactElement {
  void props;
  throw new Error("TODO(livekit): implement");
}

export function MediaPortProvider(props: { port: MediaPort; children?: ReactNode }): ReactElement {
  void props;
  throw new Error("TODO(livekit): implement");
}

/** The presence port in scope; throws naming the port when none wraps the caller. */
export function usePresencePort(): PresencePort {
  throw new Error("TODO(livekit): implement");
}

/** The media port in scope; throws naming the port when none wraps the caller. */
export function useMediaPort(): MediaPort {
  throw new Error("TODO(livekit): implement");
}

export function useHostPresence(hostId: string | null): RoomHandle | null {
  void hostId;
  throw new Error("TODO(livekit): implement");
}

export function useRoomParticipants(room: RoomHandle | null): RoomParticipant[] {
  void room;
  throw new Error("TODO(livekit): implement");
}
