import React, { useState } from "react";
import type { Client } from "@connectrpc/connect";
import { ConnectionService } from "../../gen/connection_pb";
import { useHttpClient } from "../../rpc/transportProvider";
import { Button } from "../ui/button";
import { useAgentConversation } from "./useAgentConversation";

/**
 * The body of an agent conversation tab: the transcript of the operator's exchange with one agent
 * attached to the session, and the composer that sends the next prompt.
 *
 * Not to be confused with `SessionActivitiesPane` / the Agent Activity overlay, which replay a
 * *session's* recorded ACP transcript. A roster agent has no such transcript, so this is a live
 * conversation and never a replay (PRD § What is deliberately not being built).
 *
 * Feature: docs/ft/web/session-drawer.md § Add agent; invariants: packages/tddy-web/docs/session-agent-conversation.md.
 */

export interface SessionAgentConversationPaneProps {
  readonly sessionId: string;
  readonly sessionToken: string;
  /** The daemon facilitating the session — it routes the conversation to the agent's own host. */
  readonly daemonInstanceId: string;
  /** The qualified `name@daemon_instance_id` of the agent being talked to. */
  readonly agentId: string;
  /** The id the tab is keyed by, and the id the conversation is opened and cancelled under. */
  readonly conversationId: string;
  /** Explicit client override — session-scoped routing where available. Falls back to the shared
   *  HTTP client from the transport context. */
  readonly client?: Client<typeof ConnectionService>;
}

const ROLE_NAMES: Record<"operator" | "agent", string> = {
  operator: "You",
  agent: "Agent",
};

export function SessionAgentConversationPane({
  sessionId,
  sessionToken,
  daemonInstanceId,
  agentId,
  conversationId,
  client,
}: SessionAgentConversationPaneProps) {
  // `useHttpClient` is called unconditionally (hook rules); the explicit prop wins when present.
  const httpClient = useHttpClient(ConnectionService);
  const resolvedClient = client ?? httpClient;

  const { turns, error, answering, prompt } = useAgentConversation({
    client: resolvedClient,
    sessionToken,
    sessionId,
    daemonInstanceId,
    agentId,
    conversationId,
  });

  const [draft, setDraft] = useState("");

  const send = () => {
    // An answer still arriving owns the turn it is filling: a second prompt sent into it appends an
    // operator turn *after* the incomplete agent turn, which makes the first stream's next chunk
    // open a fresh turn that the second stream then extends — two answers merged into one, with a
    // prompt stranded mid-answer. The gate lives here rather than on the button because the button
    // is not the only way in; Enter is.
    if (answering) return;
    const text = draft.trim();
    if (text === "") return;
    setDraft("");
    prompt(text);
  };

  return (
    <div
      data-testid="agent-conversation-pane"
      className="flex h-full w-full flex-col gap-2 overflow-hidden p-3 text-xs"
    >
      <div className="text-muted-foreground">{agentId}</div>

      <div
        data-testid="agent-conversation-transcript"
        className="flex min-h-0 flex-1 flex-col gap-2 overflow-y-auto"
      >
        {turns.map((turn, index) => (
          <div key={index} className="flex flex-col gap-0.5">
            {/* The speaker's name is a sibling of the turn, never inside it: the turn element
                carries the message and nothing else, so an empty answer reads as empty. */}
            <span className="text-muted-foreground">{ROLE_NAMES[turn.role]}</span>
            <span
              data-testid={`agent-conversation-turn-${index}`}
              data-role={turn.role}
              data-complete={String(turn.complete)}
              data-stop-reason={turn.stopReason}
              className="whitespace-pre-wrap break-words"
            >
              {turn.text}
            </span>
          </div>
        ))}
      </div>

      {/* A failed open, a failed prompt: named, and never shown as an empty transcript. */}
      {error !== null && (
        <p data-testid="agent-conversation-error" className="text-destructive">
          {error}
        </p>
      )}

      <div className="flex flex-shrink-0 items-center gap-2">
        <input
          type="text"
          data-testid="agent-conversation-input"
          className="min-w-0 flex-1 rounded-md border border-border bg-background px-2 py-1"
          placeholder={`Ask ${agentId}…`}
          value={draft}
          onChange={(e) => setDraft(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === "Enter") send();
          }}
        />
        <Button
          data-testid="agent-conversation-send-btn"
          size="sm"
          className="h-6 px-2 text-xs"
          // Reflects the gate in `send`, so the control says what it will do. It is not the gate.
          disabled={answering}
          onClick={send}
        >
          Send
        </Button>
      </div>
    </div>
  );
}
