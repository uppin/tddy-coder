import React from "react";
import type { SessionEntry } from "../../gen/connection_pb";
import type { SessionAttachmentHint } from "../../rpc/connections/session";
import { usePresenterLiveKitRoom } from "./usePresenterLiveKitRoom";
import { AgentChat } from "../chat/AgentChat";


export interface WorkflowChatScreenProps {
  session: SessionEntry;
  /**
   * How the attached session is reached. The chat derives its own independent presenter LiveKit
   * room connection from this (see `usePresenterLiveKitRoom`) — the same room the terminal connects
   * to, with a distinct browser participant. `SessionMainPane`'s `room` prop is VNC-purpose and
   * unused here. `null` for a session that is not attached, or one served without a room.
   */
  attachmentHint?: SessionAttachmentHint | null;
}

/**
 * Full-screen chat main-pane view for tddy-coder workflow (`tool`) sessions — every recipe except
 * `pr-stack`, which keeps its own two-pane {@link ./prstack/PrStackScreen}. Rendered in place of the
 * terminal by `resolveWorkflowView`. A single pane: the reusable {@link AgentChat} driven over the
 * session's remote Presenter via the **ACP mirror** (`acp`), matching pr-stack's chat and tddy-coder's
 * ACP-agent direction. Both transports ride the same LiveKit session connection.
 */
export function WorkflowChatScreen({
  session,
  attachmentHint = null,
}: WorkflowChatScreenProps) {
  const { room, status: roomStatus, error: roomError } = usePresenterLiveKitRoom(attachmentHint);
  const livekitServerIdentity = attachmentHint?.serverIdentity;

  return (
    <div
      data-testid="workflow-chat-screen"
      className="flex-1 min-h-0 flex flex-col overflow-hidden"
    >
      <AgentChat
        acp
        resumeSessionId={session.sessionId}
        room={room}
        livekitServerIdentity={livekitServerIdentity}
        placeholder={`Message ${session.sessionId.slice(0, 8)}…`}
        roomStatus={roomStatus}
        roomError={roomError}
      />
    </div>
  );
}
