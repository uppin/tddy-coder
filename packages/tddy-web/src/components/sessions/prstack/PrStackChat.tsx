import React, { useState } from "react";
import type { Room } from "livekit-client";
import type { SessionEntry } from "../../../gen/connection_pb";
import { Button } from "../../ui/button";
import { usePresenterChat } from "./usePresenterChat";

export interface PrStackChatProps {
  session: SessionEntry;
  room: Room | null;
  livekitServerIdentity?: string;
}

/** Chat window over the session's remote Presenter (`TddyRemote.Stream`). */
export function PrStackChat({ session, room, livekitServerIdentity }: PrStackChatProps) {
  const { messages, sendPrompt } = usePresenterChat(room, livekitServerIdentity || "server");
  const [draft, setDraft] = useState("");

  const handleSend = () => {
    const text = draft.trim();
    if (!text) return;
    sendPrompt(text);
    setDraft("");
  };

  return (
    <div data-testid="pr-stack-chat" className="flex-1 min-h-0 flex flex-col overflow-hidden">
      <div
        data-testid="pr-stack-chat-messages"
        className="flex-1 min-h-0 overflow-y-auto flex flex-col gap-2 p-3"
      >
        {messages.map((m, i) => (
          <div
            key={m.key}
            data-testid={`pr-stack-chat-message-${i}`}
            className="rounded-md bg-muted px-3 py-2 text-sm"
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
          onChange={(e) => setDraft(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === "Enter") handleSend();
          }}
        />
        <Button data-testid="pr-stack-chat-send-btn" size="sm" onClick={handleSend}>
          Send
        </Button>
      </div>
    </div>
  );
}
