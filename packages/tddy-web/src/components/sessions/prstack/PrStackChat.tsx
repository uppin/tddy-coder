import React, { useEffect, useState } from "react";
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
  const {
    messages,
    sendPrompt,
    pendingQuestion,
    answerSelect,
    answerOther,
    answerMultiSelect,
    streamError,
    sendError,
    workflowError,
  } = usePresenterChat(room, livekitServerIdentity || "server");
  const [draft, setDraft] = useState("");
  const [checkedIndices, setCheckedIndices] = useState<number[]>([]);
  const [otherDraft, setOtherDraft] = useState("");
  const isConnecting = roomStatus === "connecting";

  // Reset per-question local state (checked options, "Other" draft text) whenever a new
  // question arrives — a fresh `pendingQuestion` object is only produced on an actual mode
  // change, so keying on its identity is sufficient.
  useEffect(() => {
    setCheckedIndices([]);
    setOtherDraft("");
  }, [pendingQuestion]);

  const handleSend = () => {
    const text = draft.trim();
    if (!text) return;
    if (sendPrompt(text)) {
      setDraft("");
    }
  };

  const toggleChecked = (index: number) => {
    setCheckedIndices((prev) =>
      prev.includes(index) ? prev.filter((i) => i !== index) : [...prev, index].sort((a, b) => a - b),
    );
  };

  const handleSubmitMultiSelect = () => {
    answerMultiSelect(checkedIndices, otherDraft.trim() || undefined);
  };

  const handleSubmitOther = () => {
    answerOther(otherDraft);
  };

  const errorMessage =
    roomStatus === "error"
      ? `Presenter unavailable: ${roomError ?? "connection failed"}`
      : streamError
        ? `Presenter connection lost: ${streamError}`
        : workflowError
          ? `Workflow failed: ${workflowError}`
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
            data-message-kind={m.from}
            className={
              m.from === "user"
                ? "self-end rounded-md bg-primary text-primary-foreground px-3 py-2 text-sm"
                : m.from === "agent"
                  ? "self-start rounded-md bg-muted px-3 py-2 text-sm"
                  : m.from === "goal"
                    ? "self-center text-xs text-muted-foreground italic font-medium"
                    : "self-center text-xs text-muted-foreground italic"
            }
          >
            {m.from === "goal" ? `Goal: ${m.text}` : m.text}
          </div>
        ))}
      </div>
      {pendingQuestion ? (
        <div
          data-testid="pr-stack-chat-question"
          className="flex-shrink-0 flex flex-col gap-2 border-t border-border p-3"
        >
          <div data-testid="pr-stack-chat-question-header" className="text-xs font-medium text-muted-foreground">
            {pendingQuestion.header}
          </div>
          <div data-testid="pr-stack-chat-question-text" className="text-sm">
            {pendingQuestion.question}
          </div>
          {pendingQuestion.kind === "select" ? (
            <div className="flex flex-col gap-1.5">
              {pendingQuestion.options.map((option, i) => (
                <Button
                  key={i}
                  data-testid={`pr-stack-chat-option-${i}`}
                  variant="outline"
                  size="sm"
                  className="justify-start text-left h-auto py-1.5"
                  onClick={() => answerSelect(i)}
                >
                  {option.label}
                </Button>
              ))}
            </div>
          ) : (
            <div className="flex flex-col gap-1.5">
              {pendingQuestion.options.map((option, i) => (
                <label
                  key={i}
                  data-testid={`pr-stack-chat-multiselect-option-${i}`}
                  className="flex items-center gap-2 text-sm"
                >
                  <input
                    type="checkbox"
                    checked={checkedIndices.includes(i)}
                    onChange={() => toggleChecked(i)}
                  />
                  {option.label}
                </label>
              ))}
            </div>
          )}
          {pendingQuestion.allowOther && (
            <input
              data-testid="pr-stack-chat-question-other-input"
              type="text"
              className="rounded-md border border-input bg-background px-3 py-1.5 text-sm shadow-sm focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
              placeholder="Other…"
              value={otherDraft}
              onChange={(e) => setOtherDraft(e.target.value)}
            />
          )}
          {pendingQuestion.kind === "select" ? (
            pendingQuestion.allowOther && (
              <Button
                data-testid="pr-stack-chat-question-other-submit"
                size="sm"
                onClick={handleSubmitOther}
              >
                Submit
              </Button>
            )
          ) : (
            <Button data-testid="pr-stack-chat-multiselect-submit" size="sm" onClick={handleSubmitMultiSelect}>
              Submit
            </Button>
          )}
        </div>
      ) : (
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
      )}
    </div>
  );
}
