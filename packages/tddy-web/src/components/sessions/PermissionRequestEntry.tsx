import type { Client } from "@connectrpc/connect";
import type { SessionAgentService } from "../../gen/session_agents_pb";
import { Button } from "../ui/button";
import { useAnswerAgentPermission } from "./useAnswerAgentPermission";

/** A coding subagent's pending permission request, as the conversation transcript records it. */
export interface PendingPermission {
  requestId: string;
  title: string;
  options: { optionId: string; name: string; kind: string }[];
  /** The main agent handed it to the user — only then does the entry offer the options. */
  deferredToUser: boolean;
  /** Who answered it, once answered. */
  answeredBy?: string;
}

export interface PermissionRequestEntryProps {
  sessionId: string;
  conversationId: string;
  sessionToken: string;
  permission: PendingPermission;
  client: Client<typeof SessionAgentService>;
}

/**
 * The request's tool and, when the main agent deferred it to the user, one button per option; a
 * choice is sent with `AnswerAgentPermission` and the entry then records it and who made it.
 *
 * PRD: docs/ft/web/1-WIP/PRD-2026-10-24-coagents-web-approvals.md
 */
export function PermissionRequestEntry({
  sessionId,
  conversationId,
  sessionToken,
  permission,
  client,
}: PermissionRequestEntryProps) {
  const { answer, pending, error, recorded } = useAnswerAgentPermission({
    client,
    sessionToken,
    sessionId,
    conversationId,
  });

  // Once the daemon has accepted an answer — from this entry, or one the transcript already records —
  // the choice is final and no option is offered again.
  const answeredBy = recorded?.answeredBy ?? permission.answeredBy ?? null;
  const offerOptions = permission.deferredToUser && answeredBy === null;
  const chosen = recorded && permission.options.find((o) => o.optionId === recorded.optionId);

  return (
    <section
      data-testid="permission-request"
      className="flex flex-col gap-2 rounded-md border border-border bg-muted/50 p-3 text-xs"
    >
      <p data-testid="permission-request-title" className="font-mono font-semibold">
        {permission.title}
      </p>
      {offerOptions && (
        <div className="flex flex-wrap gap-2">
          {permission.options.map((option) => (
            <Button
              key={option.optionId}
              data-testid={`permission-option-${option.optionId}`}
              variant="outline"
              size="sm"
              className="h-6 px-2 text-xs"
              disabled={pending}
              onClick={() => void answer(permission.requestId, option.optionId)}
            >
              {option.name}
            </Button>
          ))}
        </div>
      )}
      {answeredBy !== null && (
        <p data-testid="permission-answered" className="text-muted-foreground">
          {chosen ? `${chosen.name} · answered by ${answeredBy}` : `Answered by ${answeredBy}`}
        </p>
      )}
      {error !== null && (
        <p data-testid="permission-answer-error" role="alert" className="text-destructive">
          {error}
        </p>
      )}
    </section>
  );
}
