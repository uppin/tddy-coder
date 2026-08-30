import { useCallback, useEffect, useRef, useState } from "react";
import { ConnectError, type Client } from "@connectrpc/connect";
import type { ConnectionService } from "../../gen/connection_pb";
import {
  appendAnswerChunk,
  appendOperatorTurn,
  type AgentTurn,
} from "./agentConversationTranscript";

/**
 * One live conversation with an agent attached to a session: opened on mount, prompted by the
 * operator, cancelled when the surface holding it goes away.
 *
 * This is the operator's own conversation, not a replay of the main agent's use of its sub-agents —
 * a roster agent has no session directory and no transcript to replay (see the PRD, § What is
 * deliberately not being built).
 *
 * PRD: docs/ft/web/1-WIP/PRD-2026-08-29-session-agent-conversation-tab.md (AC5-AC9).
 */

type ConnectionClient = Client<typeof ConnectionService>;

export interface AgentConversationParams {
  readonly client: ConnectionClient;
  readonly sessionToken: string;
  readonly sessionId: string;
  /** The daemon facilitating the session — it owns the roster and routes to the agent's host. */
  readonly daemonInstanceId: string;
  /** The qualified `name@daemon_instance_id` of the agent being talked to. */
  readonly agentId: string;
  /** The caller-minted id this conversation is opened, prompted and cancelled under. */
  readonly conversationId: string;
}

export interface AgentConversationState {
  /** The exchange so far, oldest first. */
  readonly turns: readonly AgentTurn[];
  /** Why the last open or prompt failed. Never stands in for a turn. */
  readonly error: string | null;
  /** True while an answer is still arriving — the composer is closed for the duration. */
  readonly answering: boolean;
  /** Send `text` to the agent and fold its answer into the transcript. */
  readonly prompt: (text: string) => void;
}

export function useAgentConversation({
  client,
  sessionToken,
  sessionId,
  daemonInstanceId,
  agentId,
  conversationId,
}: AgentConversationParams): AgentConversationState {
  const [turns, setTurns] = useState<readonly AgentTurn[]>([]);
  const [error, setError] = useState<string | null>(null);
  const [answering, setAnswering] = useState(false);

  // The routing a call needs, read through a ref so the open/cancel effect below depends on the
  // conversation's *identity* alone. A conversation is opened once and cancelled once; re-running
  // that pair because a client or a token was rebuilt would cancel a live conversation and open a
  // second one under the same id.
  const call = useRef({ client, sessionToken, sessionId, daemonInstanceId, agentId });
  call.current = { client, sessionToken, sessionId, daemonInstanceId, agentId };

  useEffect(() => {
    const { client: rpc, ...route } = call.current;
    let live = true;
    // The open is held as a promise so the cleanup can wait on it. Cancelling while it is still on
    // the wire lets the cancel land first — the daemon answers NOT_FOUND for a conversation it has
    // not created yet — and the open land after, leaving a conversation, and the agent session
    // `open_agent_conversation` spawns for it, running on the daemon with nothing left to cancel it.
    const opened = (async () => {
      try {
        await rpc.openAgentConversation({ ...route, conversationId });
        return true;
      } catch (err) {
        if (live) setError(ConnectError.from(err).rawMessage);
        return false;
      }
    })();

    return () => {
      live = false;
      void opened
        .then((wasOpened) => {
          // An open that was refused created nothing; asking the daemon to cancel it would be
          // asking it to drop a conversation it never had.
          if (!wasOpened) return undefined;
          return rpc.cancelAgentConversation({
            sessionToken: route.sessionToken,
            sessionId: route.sessionId,
            daemonInstanceId: route.daemonInstanceId,
            conversationId,
          });
        })
        // The surface that would render this failure is the one being torn down, so there is
        // nowhere to put it — but a cancel that did not land leaves an agent session running, which
        // is worth a trace rather than silence.
        .catch((err) => console.debug("[useAgentConversation] cancel failed", err));
    };
  }, [conversationId]);

  const prompt = useCallback(
    (text: string) => {
      const route = call.current;
      setError(null);
      setTurns((current) => appendOperatorTurn(current, text));
      setAnswering(true);
      void (async () => {
        try {
          for await (const chunk of route.client.promptAgentConversation({
            sessionToken: route.sessionToken,
            sessionId: route.sessionId,
            daemonInstanceId: route.daemonInstanceId,
            conversationId,
            prompt: text,
          })) {
            setTurns((current) => appendAnswerChunk(current, chunk));
          }
        } catch (err) {
          setError(ConnectError.from(err).rawMessage);
        } finally {
          setAnswering(false);
        }
      })();
    },
    [conversationId],
  );

  return { turns, error, answering, prompt };
}
