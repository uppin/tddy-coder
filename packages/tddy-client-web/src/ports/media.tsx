/**
 * Presence and media ports: rooms, their participants, and the media a room carries (camera video,
 * screen sharing, the presenter room). LiveKit is the only implementation (`tddy-livekit-web`), but
 * no component package sees a LiveKit type — a room is an opaque {@link RoomHandle}, and the port
 * supplies both the hooks that read it and the components that render its media.
 *
 * The ports carry hooks. Their identities must be stable for the app's lifetime (the app builds the
 * port once), so calling them obeys the rules of hooks like any imported hook.
 */
import { createContext, createElement, useContext, type ComponentType, type ReactElement, type ReactNode, type Ref, type RefObject } from "react";
import type { Client } from "@connectrpc/connect";
import type { DescService } from "@bufbuild/protobuf";
import type { SessionAttachmentHint } from "../connections/session";
import type { ParticipantRole } from "../lib/participantRole";

/** A joined room, opaque outside the implementation. */
export interface RoomHandle {
  readonly __room: unique symbol;
}

export type RoomStatus = "idle" | "connecting" | "connected" | "error";

/** The common room's status: the same four states every room reports. */
export type CommonRoomStatus = RoomStatus;

/** A room the caller is joining or has joined, with how far it got. */
export interface RoomState {
  readonly room: RoomHandle | null;
  readonly status: RoomStatus;
  readonly error: string | null;
}

export interface RoomParticipant {
  identity: string;
  role: ParticipantRole | (string & {});
  joinedAt: number | null;
  metadata: string;
  /** Structured Codex OAuth hint from metadata (`codex_oauth`), when present. */
  codexOAuth: { pending: boolean; authorizeUrl?: string; callbackPort?: number; state?: string } | null;
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

/**
 * RPC addressed to a participant over a room's data channel, and that participant's liveness — what
 * chat, session usage and screen-sharing input do today with `liveKitFactory(room, identity)`,
 * `room.remoteParticipants.has(identity)` and `ConnectionStateChanged`.
 */
export interface RoomRpcPort {
  /** A client for `service` addressed to `identity` in `room`; `null` until both are known and joined. Stable while they hold. */
  useRoomClient<S extends DescService>(room: RoomHandle | null, identity: string | null, service: S): Client<S> | null;
  /** Whether `identity` is in `room` now; re-renders on join, leave and reconnect. */
  useParticipantPresent(room: RoomHandle | null, identity: string | null): boolean;
  /** The room's connection state. */
  useRoomStatus(room: RoomHandle | null): RoomStatus;
}

/** What {@link MediaPort.useScreenSharingInput} forwards, and from where. */
export interface ScreenSharingInputOptions {
  /** The rendered picture. Pointer positions are read against this element's current box. */
  readonly picture: RefObject<HTMLElement | null>;
  /** The room the bridge is in; `null` until it is connected. */
  readonly room: RoomHandle | null;
  /** The participant the input stream is addressed at — this desktop's own bridge. */
  readonly bridgeIdentity: string;
  /** The remote framebuffer's size, which is what the events are expressed in. */
  readonly framebufferWidth: number;
  readonly framebufferHeight: number;
  /** Called when the operator presses the one chord the overlay keeps for itself. */
  readonly onCloseChord: () => void;
}

/** What {@link MediaPort.RemoteVideo} renders, and the handles a parent may hold on the picture. */
export interface RemoteVideoProps {
  readonly room: RoomHandle | null;
  readonly identity: string;
  readonly trackName: string;
  readonly className?: string;
  readonly testId?: string;
  /** The `<video>` element, from the first render — what {@link ScreenSharingInputOptions.picture} points at. */
  readonly videoRef?: Ref<HTMLVideoElement>;
}

/** What {@link MediaPort.VideoPreview} renders: one participant's camera, in a dialog. */
export interface ParticipantVideoPreviewProps {
  readonly room: RoomHandle | null;
  readonly identity: string;
  readonly open: boolean;
  readonly onOpenChange: (open: boolean) => void;
}

export interface MediaPort {
  /** Join the presenter room a session's attachment hint names; no room when the hint names none. */
  usePresenterRoom(hint: SessionAttachmentHint | null): RoomState;
  /**
   * Forward pointer and keyboard input on `picture` to a screen-sharing bridge in `room`.
   *
   * `inputUnavailable` is true once there is no input stream to send on — the desktop can be
   * watched but not touched: the peer does not serve the input service at all, or a stream that was
   * open ended on its own. It is not set for a desktop that silently drops injection.
   */
  useScreenSharingInput(options: ScreenSharingInputOptions): { readonly inputUnavailable: boolean };
  /** Whether `identity` has camera video worth offering a preview of in `room`. */
  useHasCameraVideo(room: RoomHandle | null, identity: string): boolean;
  /** Renders the video track `trackName` of `identity` in `room`. */
  readonly RemoteVideo: ComponentType<RemoteVideoProps>;
  /** The camera preview a roster opens for one participant. */
  readonly VideoPreview: ComponentType<ParticipantVideoPreviewProps>;
}

const PresencePortContext = createContext<PresencePort | null>(null);
const RoomRpcPortContext = createContext<RoomRpcPort | null>(null);
const MediaPortContext = createContext<MediaPort | null>(null);

export function PresencePortProvider(props: { port: PresencePort; children?: ReactNode }): ReactElement {
  return createElement(PresencePortContext.Provider, { value: props.port }, props.children);
}

export function RoomRpcPortProvider(props: { port: RoomRpcPort; children?: ReactNode }): ReactElement {
  return createElement(RoomRpcPortContext.Provider, { value: props.port }, props.children);
}

export function MediaPortProvider(props: { port: MediaPort; children?: ReactNode }): ReactElement {
  return createElement(MediaPortContext.Provider, { value: props.port }, props.children);
}

function required<T>(port: T | null, name: string): T {
  if (port === null) throw new Error(`${name} is not provided: wrap the caller in <${name}Provider> (the app registers it through its LiveKit ports)`);
  return port;
}

/** The presence port in scope; throws naming the port when none wraps the caller. */
export function usePresencePort(): PresencePort {
  return required(useContext(PresencePortContext), "PresencePort");
}

/** The room-RPC port in scope; throws naming the port when none wraps the caller. */
export function useRoomRpcPort(): RoomRpcPort {
  return required(useContext(RoomRpcPortContext), "RoomRpcPort");
}

/** The media port in scope; throws naming the port when none wraps the caller. */
export function useMediaPort(): MediaPort {
  return required(useContext(MediaPortContext), "MediaPort");
}

export function useRoomClient<S extends DescService>(room: RoomHandle | null, identity: string | null, service: S): Client<S> | null {
  return useRoomRpcPort().useRoomClient(room, identity, service);
}

export function useParticipantPresent(room: RoomHandle | null, identity: string | null): boolean {
  return useRoomRpcPort().useParticipantPresent(room, identity);
}

export function useRoomStatus(room: RoomHandle | null): RoomStatus {
  return useRoomRpcPort().useRoomStatus(room);
}

export function useHostPresence(hostId: string | null): RoomHandle | null {
  return usePresencePort().useHostPresence(hostId);
}

export function usePresenceRoom(): RoomHandle | null {
  return usePresencePort().usePresenceRoom();
}

export function useRoomParticipants(room: RoomHandle | null): RoomParticipant[] {
  return usePresencePort().useRoomParticipants(room);
}
