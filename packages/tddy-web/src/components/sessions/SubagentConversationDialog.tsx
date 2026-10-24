import { useEffect, useState } from "react";
import type { Client } from "@connectrpc/connect";
import type { ActivityService } from "../../gen/activity_pb";
import { SessionAgentService } from "../../gen/session_agents_pb";
import { useHttpClient } from "../../rpc/transportProvider";
import { AgentChatView } from "../chat/AgentChat";
import { useAcpReplay } from "../chat/useAcpReplay";
import type { ChatMessage } from "../chat/useAgentChat";
import { AgentActivityDetailDialog } from "./AgentActivityDetailDialog";
import { permissionRequestIdOf } from "./pendingPermission";
import { TranscriptPermissionRequest } from "./TranscriptPermissionRequest";

export interface SubagentConversationDialogProps {
  sessionId: string;
  /** The conversation the clicked bubble stands for. */
  conversationId: string;
  /** The bubble's text — the agent and what it was asked — shown as the dialog's title. */
  title: string;
  sessionToken: string;
  /** The daemon that owns the session (`SessionEntry.daemonInstanceId`); empty when not known. A
   *  permission request's answer is addressed to it. */
  daemonInstanceId: string;
  client: Client<typeof ActivityService>;
  onClose: () => void;
}

/** Shown in place of the transcript when the conversation recorded nothing. It states a fact about the
 *  record — an in-jail conversation is never written down — rather than reporting a failed lookup. */
const NOT_RECORDED_TEXT =
  "This conversation was not recorded: it ran inside its jail, and its history exists only there.";

/**
 * A subagent conversation itself, opened from its activity bubble: the conversation's own ACP
 * transcript in the activity pane's read-only view (`AgentChatView readOnly` over `useAcpReplay`
 * with the conversation's id), live while it runs, its tool calls opening `AgentActivityDetailDialog`
 * like the pane's. A conversation that recorded nothing (an in-jail loop) says so. A permission request
 * the conversation recorded is answered from its own entry beneath the transcript.
 *
 * PRD: docs/ft/web/1-WIP/PRD-2026-10-24-coagents-conversation-bubble.md
 */
export function SubagentConversationDialog({
  sessionId,
  conversationId,
  title,
  sessionToken,
  daemonInstanceId,
  client,
  onClose,
}: SubagentConversationDialogProps) {
  const chat = useAcpReplay({ sessionId, sessionToken, client, conversationId });
  const { loadSnapshot, notRecorded, atOldest, loadingOlder, loadOlder, messages } = chat;
  const [detail, setDetail] = useState<ChatMessage | null>(null);
  // Answering a request is a session-agent call, made on the shared transport like the roster's.
  const agentClient = useHttpClient(SessionAgentService);
  // Each permission request the conversation recorded gets its own entry, read from its transcript.
  const permissionRequests = messages.filter(
    (m) => m.toolCallId !== undefined && permissionRequestIdOf(m.toolCallId) !== null,
  );

  // Opening the dialog pulls the conversation's transcript; the registry keeps it for a reopen.
  useEffect(() => {
    loadSnapshot();
  }, [loadSnapshot]);

  // Escape closes this dialog, but not while a tool call's detail is open over it: that one answers
  // the key first.
  useEffect(() => {
    if (detail) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        e.preventDefault();
        onClose();
      }
    };
    document.addEventListener("keydown", onKey);
    return () => document.removeEventListener("keydown", onKey);
  }, [detail, onClose]);

  return (
    <div
      className="fixed inset-0 z-50 flex items-center justify-center bg-black/50 p-4"
      role="presentation"
      onMouseDown={(e) => {
        if (e.target === e.currentTarget) onClose();
      }}
    >
      <div
        data-testid="subagent-conversation-dialog"
        role="dialog"
        aria-modal="true"
        aria-labelledby="subagent-conversation-title"
        // A definite height, not a maximum: the transcript inside scrolls only within a bounded box.
        className="flex h-[min(80vh,720px)] w-full max-w-3xl flex-col overflow-hidden rounded-lg border border-border bg-background shadow-lg"
        onMouseDown={(e) => e.stopPropagation()}
      >
        <header className="flex shrink-0 items-center justify-between border-b border-border px-4 py-3">
          <h2 id="subagent-conversation-title" className="font-mono text-sm font-semibold">
            {title}
          </h2>
          <button
            type="button"
            className="rounded-md px-2 py-1 text-sm text-muted-foreground hover:bg-muted"
            onClick={onClose}
            data-testid="subagent-conversation-close"
          >
            Close
          </button>
        </header>
        {notRecorded ? (
          <p
            data-testid="subagent-conversation-not-recorded"
            role="status"
            className="p-4 text-sm text-muted-foreground"
          >
            {NOT_RECORDED_TEXT}
          </p>
        ) : (
          <AgentChatView
            room={null}
            readOnly
            chat={chat}
            onToolClick={setDetail}
            onLoadOlder={loadOlder}
            hasOlder={!atOldest}
            loadingOlder={loadingOlder}
          />
        )}
        {permissionRequests.length > 0 && (
          <section
            data-testid="subagent-permission-requests"
            aria-label="Permission requests"
            className="flex max-h-56 shrink-0 flex-col gap-2 overflow-y-auto border-t border-border p-3"
          >
            {permissionRequests.map((message) => (
              <TranscriptPermissionRequest
                key={message.key}
                message={message}
                sessionId={sessionId}
                sessionToken={sessionToken}
                daemonInstanceId={daemonInstanceId}
                conversationId={conversationId}
                activityClient={client}
                agentClient={agentClient}
              />
            ))}
          </section>
        )}
        {detail && (
          <AgentActivityDetailDialog
            message={detail}
            sessionId={sessionId}
            sessionToken={sessionToken}
            client={client}
            conversationId={conversationId}
            onClose={() => setDetail(null)}
          />
        )}
      </div>
    </div>
  );
}
