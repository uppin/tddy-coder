/**
 * An orchestrator's requests for access to paths beyond its reach, in its session pane, each with
 * Allow and Deny (`#orchestra` 6/9). The daemon holds the requests (`ListPathAccessRequests`); an
 * answer is `AnswerPathAccess`, after which the request leaves the list. Allowing grants the paths
 * for the rest of the session; the agent's waiting `RequestPathAccess` returns with the answer.
 *
 * Feature doc: docs/ft/daemon/project-concept.md § Orchestrator sessions.
 */

import React, { useCallback, useEffect, useRef, useState } from "react";
import { Code, ConnectError, type Client } from "@connectrpc/connect";
import { rpcFailureText } from "tddy-client-web";
import { Button, InlineError } from "tddy-ui-web";
import type { ActivityService, PathAccessRequest } from "tddy-client-web/gen/activity_pb";

/**
 * How often the daemon is asked for the session's waiting requests. Polled rather than driven by
 * the `path_access` notification: the notification feed is the drawer's one daemon-wide stream, and
 * nothing is announced when a request is answered elsewhere or times out, so only a re-read sees it
 * leave.
 */
const POLL_MS = 3000;

export interface PathAccessRequestsProps {
  /** The orchestrator session whose requests are shown. */
  sessionId: string;
  sessionToken: string;
  client: Client<typeof ActivityService>;
}

/** The pending requests, oldest first; renders nothing when there are none. */
export function PathAccessRequests({ sessionId, sessionToken, client }: PathAccessRequestsProps) {
  const [requests, setRequests] = useState<PathAccessRequest[]>([]);
  const [answerError, setAnswerError] = useState<string | null>(null);
  // Answered ids stay hidden even when a poll that was already in flight lists them again.
  const answered = useRef(new Set<string>());

  const refresh = useCallback(async () => {
    try {
      const listed = await client.listPathAccessRequests({ sessionToken, sessionId });
      setRequests(listed.requests.filter((r) => !answered.current.has(r.requestId)));
    } catch {
      // The daemon is unreachable or the session is gone: keep what is shown; the next poll retries.
    }
  }, [client, sessionToken, sessionId]);

  useEffect(() => {
    answered.current = new Set();
    setRequests([]);
    setAnswerError(null);
    void refresh();
    const timer = setInterval(() => void refresh(), POLL_MS);
    return () => clearInterval(timer);
  }, [refresh]);

  const answer = async (requestId: string, allow: boolean) => {
    try {
      await client.answerPathAccess({ sessionToken, sessionId, requestId, allow });
    } catch (err) {
      // Not found means it was taken already (answered elsewhere or timed out), so it leaves the
      // list either way. Any other failure left it waiting: it stays, and the operator is told.
      if (ConnectError.from(err).code !== Code.NotFound) {
        setAnswerError(rpcFailureText(err));
        return;
      }
    }
    setAnswerError(null);
    answered.current.add(requestId);
    setRequests((current) => current.filter((r) => r.requestId !== requestId));
  };

  if (requests.length === 0) return null;
  return (
    <div data-testid="path-access-requests" className="flex flex-col gap-2 border-b border-border p-2">
      {answerError && (
        <InlineError testId="path-access-answer-error" message={answerError} className="text-xs" />
      )}
      {requests.map((request) => (
        <div
          key={request.requestId}
          data-testid={`path-access-request-${request.requestId}`}
          className="flex flex-col gap-1 rounded border border-border bg-muted/40 p-2 text-sm"
        >
          <div>
            The orchestrator asks for <strong>{request.mode}</strong> access to:
          </div>
          <ul className="font-mono text-xs">
            {request.paths.map((path) => (
              <li key={path}>{path}</li>
            ))}
          </ul>
          <div className="text-muted-foreground">{request.reason}</div>
          <div className="flex gap-2">
            <Button
              size="sm"
              data-testid={`path-access-allow-${request.requestId}`}
              onClick={() => void answer(request.requestId, true)}
            >
              Allow
            </Button>
            <Button
              size="sm"
              variant="outline"
              data-testid={`path-access-deny-${request.requestId}`}
              onClick={() => void answer(request.requestId, false)}
            >
              Deny
            </Button>
          </div>
        </div>
      ))}
    </div>
  );
}
