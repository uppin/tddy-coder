import React, { useEffect, useState } from "react";
import type { Room } from "livekit-client";
import type { CommonRoomStatus } from "../../hooks/useCommonRoom";
import { Button } from "../ui/button";
import { useAgentChat, type UseAgentChatResult } from "./useAgentChat";
import { useAcpSession } from "./useAcpSession";
import { buildChatTranscript, downloadTextFile } from "./chatTranscript";

const STATUS_LABEL: Record<CommonRoomStatus, string> = {
  idle: "Not connected",
  connecting: "Connecting…",
  connected: "Connected",
  error: "Disconnected",
};

export interface AgentChatProps {
  room: Room | null;
  livekitServerIdentity?: string;
  /** Placeholder shown in the free-text input. Defaults to a generic "Message the agent…". */
  placeholder?: string;
  /** Status of the presenter LiveKit room connection the caller has established for this chat. */
  roomStatus?: CommonRoomStatus;
  /** Error from the room connection attempt — only meaningful when `roomStatus === "error"`. */
  roomError?: string | null;
  /** Drive the session over the ACP protobuf mirror (`AcpService.Session`) instead of the default
   *  `TddyRemote.Stream`. Both ride the same LiveKit session connection and render identically. */
  acp?: boolean;
}

/**
 * Recipe-agnostic chat window over a session's remote agent. By default it speaks the Presenter's
 * `TddyRemote.Stream`; with `acp`, it speaks the ACP protobuf mirror (`AcpService.Session`) over the
 * same LiveKit session connection. Hook selection lives in the two backed wrappers below so neither
 * hook is called conditionally; the presentation (`AgentChatView`) is shared.
 */
export function AgentChat(props: AgentChatProps) {
  return props.acp ? <AcpBackedChat {...props} /> : <RemoteBackedChat {...props} />;
}

function RemoteBackedChat(props: AgentChatProps) {
  const chat = useAgentChat(props.room, props.livekitServerIdentity || "server");
  return <AgentChatView {...props} chat={chat} />;
}

function AcpBackedChat(props: AgentChatProps) {
  const chat = useAcpSession(props.room, props.livekitServerIdentity || "server");
  return <AgentChatView {...props} chat={chat} />;
}

export function AgentChatView({
  placeholder,
  roomStatus = "idle",
  roomError = null,
  chat,
}: AgentChatProps & { chat: UseAgentChatResult }) {
  const {
    messages,
    elicitations,
    sendPrompt,
    pendingQuestion,
    answerSelect,
    answerOther,
    answerMultiSelect,
    streamError,
    sendError,
    workflowError,
  } = chat;
  const [draft, setDraft] = useState("");

  const handleExport = () => {
    const stamp = new Date().toISOString().replace(/[:.]/g, "-");
    downloadTextFile(`chat-transcript-${stamp}.txt`, buildChatTranscript(messages, elicitations));
  };
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
    <div data-testid="agent-chat" className="relative flex-1 min-h-0 flex flex-col overflow-hidden">
      <div className="flex-shrink-0 flex items-center justify-between px-3 pt-2">
        <span data-testid="agent-chat-status" className="text-xs text-muted-foreground">
          {STATUS_LABEL[roomStatus]}
        </span>
        <Button
          data-testid="agent-chat-export-btn"
          variant="ghost"
          size="sm"
          className="h-6 px-2 text-xs"
          onClick={handleExport}
          disabled={messages.length === 0 && elicitations.length === 0}
          title="Export the chat transcript (with timestamps and clarification points) as a .txt file"
        >
          Export
        </Button>
      </div>
      {isConnecting && (
        <div
          data-testid="agent-chat-connecting"
          className="absolute inset-0 z-10 flex items-center justify-center bg-background/80 backdrop-blur-sm text-sm text-muted-foreground"
        >
          Connecting to presenter…
        </div>
      )}
      {errorMessage && (
        <p data-testid="agent-chat-error" role="alert" className="text-xs text-destructive px-3 pt-2">
          {errorMessage}
        </p>
      )}
      <div
        data-testid="agent-chat-messages"
        className="flex-1 min-h-0 overflow-y-auto flex flex-col gap-2 p-3"
      >
        {messages.map((m, i) => (
          <div
            key={m.key}
            data-testid={`agent-chat-message-${i}`}
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
          data-testid="agent-chat-question"
          className="flex-shrink-0 flex flex-col gap-2 border-t border-border p-3"
        >
          <div data-testid="agent-chat-question-header" className="text-xs font-medium text-muted-foreground">
            {pendingQuestion.header}
          </div>
          <div data-testid="agent-chat-question-text" className="text-sm">
            {pendingQuestion.question}
          </div>
          {pendingQuestion.kind === "select" ? (
            <div className="flex flex-col gap-1.5">
              {pendingQuestion.options.map((option, i) => (
                <Button
                  key={i}
                  data-testid={`agent-chat-option-${i}`}
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
                  data-testid={`agent-chat-multiselect-option-${i}`}
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
              data-testid="agent-chat-question-other-input"
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
                data-testid="agent-chat-question-other-submit"
                size="sm"
                onClick={handleSubmitOther}
              >
                Submit
              </Button>
            )
          ) : (
            <Button data-testid="agent-chat-multiselect-submit" size="sm" onClick={handleSubmitMultiSelect}>
              Submit
            </Button>
          )}
        </div>
      ) : (
        <div className="flex-shrink-0 flex gap-2 border-t border-border p-2">
          <input
            data-testid="agent-chat-input"
            type="text"
            className="flex-1 rounded-md border border-input bg-background px-3 py-1.5 text-sm shadow-sm focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
            placeholder={placeholder ?? "Message the agent…"}
            value={draft}
            disabled={isConnecting}
            onChange={(e) => setDraft(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === "Enter") handleSend();
            }}
          />
          <Button data-testid="agent-chat-send-btn" size="sm" onClick={handleSend} disabled={isConnecting}>
            Send
          </Button>
        </div>
      )}
    </div>
  );
}
