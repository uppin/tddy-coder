import type { Client } from "@connectrpc/connect";
import type { ActivityService } from "../../gen/activity_pb";
import type { SessionAgentService } from "../../gen/session_agents_pb";
import type { ChatMessage } from "../chat/useAgentChat";
import { useAcpToolCallDetail } from "../chat/useAcpToolCallDetail";
import { PermissionRequestEntry } from "./PermissionRequestEntry";
import { permissionRequestOf } from "./pendingPermission";

export interface TranscriptPermissionRequestProps {
  /** The transcript entry that records the request: a tool call whose id is `permission:<requestId>`. */
  message: ChatMessage;
  sessionId: string;
  sessionToken: string;
  /** The daemon that owns the session (`SessionEntry.daemonInstanceId`); empty when not known. */
  daemonInstanceId: string;
  /** The subagent conversation the request was parked in. */
  conversationId: string;
  /** Reads the request's input from the conversation's transcript, as any tool call's bodies are read. */
  activityClient: Client<typeof ActivityService>;
  /** Answers the request. */
  agentClient: Client<typeof SessionAgentService>;
}

/**
 * The entry for one permission request a subagent conversation recorded. The request is read from its
 * tool call's input, fetched by the entry's `tool_call_id` the same way a tool call's bodies are, so
 * the conversation view needs nothing from the bubble beyond the transcript itself.
 *
 * A request whose input cannot be read, or cannot be fetched, is said so in place of its entry.
 *
 * TODO(web-approvals): the answer the daemon accepted is not recorded in the transcript yet (the
 * answered frame is the recording node's). A reopened dialog therefore offers the options again, and
 * the daemon refuses the second answer, naming who gave the first.
 */
export function TranscriptPermissionRequest({
  message,
  sessionId,
  sessionToken,
  daemonInstanceId,
  conversationId,
  activityClient,
  agentClient,
}: TranscriptPermissionRequestProps) {
  // A request still waiting is not final, so its input is read again on every open rather than cached.
  const detail = useAcpToolCallDetail({
    sessionId,
    sessionToken,
    client: activityClient,
    toolCallId: message.toolCallId ?? "",
    cacheable: false,
    conversationId,
  });

  if (detail.status === "loading") {
    return (
      <p
        data-testid="permission-request-loading"
        role="status"
        className="text-xs text-muted-foreground"
      >
        Loading permission request…
      </p>
    );
  }
  if (detail.status === "error") {
    return (
      <p data-testid="permission-request-error" role="alert" className="text-xs text-destructive">
        Could not load this permission request.
      </p>
    );
  }
  const permission = permissionRequestOf(detail.rawInput);
  if (permission === null) {
    return (
      <p
        data-testid="permission-request-unreadable"
        role="alert"
        className="text-xs text-destructive"
      >
        This permission request&apos;s details could not be read.
      </p>
    );
  }
  return (
    <PermissionRequestEntry
      sessionId={sessionId}
      conversationId={conversationId}
      sessionToken={sessionToken}
      daemonInstanceId={daemonInstanceId}
      permission={permission}
      client={agentClient}
    />
  );
}
