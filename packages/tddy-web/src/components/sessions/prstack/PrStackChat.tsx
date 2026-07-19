import React from "react";
import type { Room } from "livekit-client";
import type { SessionEntry } from "../../../gen/connection_pb";
import type { CommonRoomStatus } from "../../../hooks/useCommonRoom";
import { AgentChat } from "../../chat/AgentChat";

export interface PrStackChatProps {
  session: SessionEntry;
  room: Room | null;
  livekitServerIdentity?: string;
  /** Status of the screen's own dedicated presenter LiveKit room connection (see `usePresenterLiveKitRoom`). */
  roomStatus?: CommonRoomStatus;
  /** Error from the room connection attempt — only meaningful when `roomStatus === "error"`. */
  roomError?: string | null;
}

/**
 * PR-Stack-specific adapter over the reusable {@link AgentChat}: derives the input placeholder from
 * the session id and forwards the presenter room wiring. The chat behavior itself now lives in the
 * recipe-agnostic `AgentChat` / `useAgentChat`.
 */
export function PrStackChat({
  session,
  room,
  livekitServerIdentity,
  roomStatus = "idle",
  roomError = null,
}: PrStackChatProps) {
  return (
    <AgentChat
      room={room}
      livekitServerIdentity={livekitServerIdentity}
      placeholder={`Message ${session.sessionId.slice(0, 8)}…`}
      roomStatus={roomStatus}
      roomError={roomError}
    />
  );
}
