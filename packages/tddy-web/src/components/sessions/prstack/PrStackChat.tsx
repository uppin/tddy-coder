import React, { useState } from "react";
import type { Room } from "livekit-client";
import type { SessionEntry } from "../../../gen/connection_pb";
import type { CommonRoomStatus } from "../../../hooks/useCommonRoom";
import { Button } from "../../ui/button";
import { usePresenterChat } from "./usePresenterChat";

const STATUS_LABEL: Record<CommonRoomStatus, string> = {
  idle: "Not connected",
  connecting: "Connecting…",
  connected: "Connected",
  error: "Disconnected",
};

export interface PrStackChatProps {
  session: SessionEntry;
  room: Room | null;
  livekitServerIdentity?: string;
  /** Status of the screen's own dedicated presenter LiveKit room connection (see `usePresenterLiveKitRoom`). */
  roomStatus?: CommonRoomStatus;
  /** Error from the room connection attempt — only meaningful when `roomStatus === "error"`. */
  roomError?: string | null;
}

/** Chat window over the session's remote Presenter (`TddyRemote.Stream`). */
export function PrStackChat({
  session,
  room,
  livekitServerIdentity,
  roomStatus = "idle",
  roomError = null,
}: PrStackChatProps) {
  const { messages, sendPrompt, streamError, sendError } = usePresenterChat(
    room,
    livekitServerIdentity || "server",
  );
  const [draft, setDraft] = useState("");
  const isConnecting = roomStatus === "connecting";

  const handleSend = () => {
    const text = draft.trim();
    if (!text) return;
    if (sendPrompt(text)) {
      setDraft("");
    }
  };

  const errorMessage =
    roomStatus === "error"
      ? `Presenter unavailable: ${roomError ?? "connection failed"}`
      : streamError
        ? `Presenter connection lost: ${streamError}`
        : sendError;

  return (
    <div data-testid="pr-stack-chat" className="relative flex-1 min-h-0 flex flex-col overflow-hidden">
      <div
        data-testid="pr-stack-chat-status"
        className="flex-shrink-0 px-3 pt-2 text-xs text-muted-foreground"
      >
        {STATUS_LABEL[roomStatus]}
      </div>
      {isConnecting && (
        <div
          data-testid="pr-stack-chat-connecting"
          className="absolute inset-0 z-10 flex items-center justify-center bg-background/80 backdrop-blur-sm text-sm text-muted-foreground"
        >
          Connecting to presenter…
        </div>
      )}
      {errorMessage && (
        <p data-testid="pr-stack-chat-error" role="alert" className="text-xs text-destructive px-3 pt-2">
          {errorMessage}
        </p>
      )}
      <div
        data-testid="pr-stack-chat-messages"
        className="flex-1 min-h-0 overflow-y-auto flex flex-col gap-2 p-3"
      >
        {messages.map((m, i) => (
          <div
            key={m.key}
            data-testid={`pr-stack-chat-message-${i}`}
            className={
              m.from === "user"
                ? "self-end rounded-md bg-primary text-primary-foreground px-3 py-2 text-sm"
                : "self-start rounded-md bg-muted px-3 py-2 text-sm"
            }
          >
            {m.text}
          </div>
        ))}
      </div>
      <div className="flex-shrink-0 flex gap-2 border-t border-border p-2">
        <input
          data-testid="pr-stack-chat-input"
          type="text"
          className="flex-1 rounded-md border border-input bg-background px-3 py-1.5 text-sm shadow-sm focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
          placeholder={`Message ${session.sessionId.slice(0, 8)}…`}
          value={draft}
          disabled={isConnecting}
          onChange={(e) => setDraft(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === "Enter") handleSend();
          }}
        />
        <Button data-testid="pr-stack-chat-send-btn" size="sm" onClick={handleSend} disabled={isConnecting}>
          Send
        </Button>
      </div>
    </div>
  );
}
