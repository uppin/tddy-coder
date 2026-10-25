import { useCallback, useState } from "react";
import { ConnectError, type Client } from "@connectrpc/connect";
import type { SessionAgentService } from "../../gen/session_agents_pb";
import type { RosterHalf } from "./sessionRosterHalf";

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

/** What an entry says when its owning daemon is not known: nothing can be answered from here. */
export const OWNING_DAEMON_UNKNOWN_TEXT =
  "The daemon that owns this session is not known, so this request cannot be answered from here.";

/** Whether a session's owning daemon id is known. An empty id is "not known", never "this daemon":
 *  the answer is addressed to the daemon holding the conversation, so none is sent without one. */
export function owningDaemonKnown(daemonInstanceId: string): boolean {
  return daemonInstanceId.trim() !== "";
}

export interface AnswerAgentPermissionParams {
  readonly client: Client<typeof SessionAgentService>;
  readonly sessionToken: string;
  /** The roster half of the session the conversation is open on: the daemon that holds the
   *  conversation, and the session it is held under. An empty daemon id is "not known". */
  readonly rosterHalf: RosterHalf;
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
  rosterHalf,
  conversationId,
}: AnswerAgentPermissionParams): AnswerAgentPermissionState {
  const [pending, setPending] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [recorded, setRecorded] = useState<RecordedPermissionAnswer | null>(null);
  const { sessionId, daemonInstanceId } = rosterHalf;

  const answer = useCallback(
    async (requestId: string, optionId: string) => {
      // Without the owning daemon the call would be addressed to "", the daemon this browser is on,
      // which is not necessarily the one holding the conversation. Nothing is sent in its place.
      if (!owningDaemonKnown(daemonInstanceId)) {
        setError(OWNING_DAEMON_UNKNOWN_TEXT);
        return;
      }
      setPending(true);
      setError(null);
      try {
        const response = await client.answerAgentPermission({
          sessionToken,
          sessionId,
          daemonInstanceId,
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
    [client, sessionToken, sessionId, daemonInstanceId, conversationId],
  );

  return { answer, pending, error, recorded };
}
