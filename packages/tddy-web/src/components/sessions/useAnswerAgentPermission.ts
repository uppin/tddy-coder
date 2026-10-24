import { useCallback, useState } from "react";
import { ConnectError, type Client } from "@connectrpc/connect";
import type { SessionAgentService } from "../../gen/session_agents_pb";

/**
 * Answers one permission request a subagent conversation parked, through
 * `SessionAgentService.AnswerAgentPermission`.
 *
 * The daemon records the first answer and refuses every later one, so what this hook keeps is the
 * answer the daemon accepted for this caller: the option sent and the login it recorded it under. A
 * refusal is kept as the daemon's own words and is never turned into an answer.
 *
 * PRD: docs/ft/web/1-WIP/PRD-2026-10-24-coagents-web-approvals.md
 */

export interface AnswerAgentPermissionParams {
  readonly client: Client<typeof SessionAgentService>;
  readonly sessionToken: string;
  readonly sessionId: string;
  /** The conversation the request was parked in. */
  readonly conversationId: string;
}

/** The answer the daemon accepted from this caller. */
export interface RecordedPermissionAnswer {
  readonly optionId: string;
  readonly answeredBy: string;
}

export interface AnswerAgentPermissionState {
  /** Send `optionId` as the answer to `requestId`. Resolves once the daemon has answered. */
  readonly answer: (requestId: string, optionId: string) => Promise<void>;
  /** True while the answer is on the wire. */
  readonly pending: boolean;
  /** The daemon's refusal of the last answer, or `null`. */
  readonly error: string | null;
  /** The answer the daemon accepted from this caller, once it has. */
  readonly recorded: RecordedPermissionAnswer | null;
}

export function useAnswerAgentPermission({
  client,
  sessionToken,
  sessionId,
  conversationId,
}: AnswerAgentPermissionParams): AnswerAgentPermissionState {
  const [pending, setPending] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [recorded, setRecorded] = useState<RecordedPermissionAnswer | null>(null);

  const answer = useCallback(
    async (requestId: string, optionId: string) => {
      setPending(true);
      setError(null);
      try {
        const response = await client.answerAgentPermission({
          sessionToken,
          sessionId,
          // An empty daemon instance id means "serve here", the routing the transcript calls use.
          // TODO(web-approvals): a session hosted by another daemon needs that daemon's id here
          // (`rosterHalfOf` routes roster calls by it). The entry's props carry none yet.
          daemonInstanceId: "",
          conversationId,
          requestId,
          optionId,
          cancelled: false,
        });
        setRecorded({ optionId, answeredBy: response.answeredBy });
      } catch (err) {
        setError(ConnectError.from(err).rawMessage);
      } finally {
        setPending(false);
      }
    },
    [client, sessionToken, sessionId, conversationId],
  );

  return { answer, pending, error, recorded };
}
