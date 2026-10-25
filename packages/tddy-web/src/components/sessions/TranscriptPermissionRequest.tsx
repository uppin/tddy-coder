import type { Client } from "@connectrpc/connect";
import type { ActivityService } from "../../gen/activity_pb";
import type { SessionAgentService } from "../../gen/session_agents_pb";
import type { ChatMessage } from "../chat/useAgentChat";
import { useAcpToolCallDetail } from "../chat/useAcpToolCallDetail";
import type { PendingPermission } from "./PermissionRequestEntry";
import { PermissionRequestEntry } from "./PermissionRequestEntry";
import { answerFieldsOf, permissionRequestOf } from "./pendingPermission";
import type { RosterHalf } from "./sessionRosterHalf";

export interface TranscriptPermissionRequestProps {
  /** The transcript entry that records the request: a tool call whose id is `permission:<requestId>`. */
  message: ChatMessage;
  /** The session the conversation is read under. */
  sessionId: string;
  sessionToken: string;
  /** The roster half the request's answer is addressed to: the daemon holding the conversation. */
  rosterHalf: RosterHalf;
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
 * The answer the daemon accepted is read from the same entry: once the call completed, its raw
 * output is the answer, so a request answered before this conversation was opened (or on another
 * view) shows who answered it and offers no option. A request whose input cannot be read, or cannot
 * be fetched, is said so in place of its entry.
 */
export function TranscriptPermissionRequest({
  message,
  sessionId,
  sessionToken,
  rosterHalf,
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
  // Only a completed call carries the answer. A call still running has none yet, whatever its
  // output holds, so the request stays open and its options stay offered.
  const answer = message.toolStatus === "completed" ? answerFieldsOf(detail.rawOutput) : null;
  const answered: PendingPermission = answer === null ? permission : { ...permission, ...answer };
  return (
    <PermissionRequestEntry
      conversationId={conversationId}
      sessionToken={sessionToken}
      rosterHalf={rosterHalf}
      permission={answered}
      client={agentClient}
    />
  );
}
