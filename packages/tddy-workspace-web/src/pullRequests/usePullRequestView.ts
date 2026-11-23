/**
 * Streaming hook for the session inspector's PR tab: `WorktreeService.StreamSessionPullRequest`, the
 * daemon's view of the PR whose head is the session's branch, fed by its GitHub inbox.
 *
 * A stream that fails or ends is reported as `error` (the last view stays beside it) and re-subscribed
 * with a doubling delay; the error clears on the first frame of the new subscription. No client at all
 * is reported, not retried.
 */
import { useEffect, useState } from "react";
import type { Client } from "@connectrpc/connect";
import type { SessionPullRequestView, WorktreeService } from "tddy-client-web/gen/worktree_pb";
import { hostNotConnectedText, rpcFailureText } from "tddy-client-web";

const FIRST_RETRY_MS = 1000;
const LONGEST_RETRY_MS = 30_000;

export interface UsePullRequestViewResult {
  /** The latest view the daemon sent, or `null` before the first frame. */
  view: SessionPullRequestView | null;
  /** Why the stream is down (or that no client reaches the host); `null` while it is live. */
  error: string | null;
}

export function usePullRequestView(
  client: Client<typeof WorktreeService> | null,
  sessionToken: string,
  projectId: string,
  sessionId: string,
): UsePullRequestViewResult {
  const [view, setView] = useState<SessionPullRequestView | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    setView(null);
    if (!client) {
      setError(hostNotConnectedText(null));
      return;
    }
    setError(null);
    const abort = new AbortController();
    let retryTimer: ReturnType<typeof setTimeout> | undefined;
    let delay = FIRST_RETRY_MS;

    const subscribe = async () => {
      try {
        for await (const frame of client.streamSessionPullRequest(
          { sessionToken, projectId, sessionId },
          { signal: abort.signal },
        )) {
          setView(frame);
          setError(null);
          delay = FIRST_RETRY_MS;
        }
        if (!abort.signal.aborted) setError("The pull request stream ended");
      } catch (err) {
        if (abort.signal.aborted) return;
        setError(rpcFailureText(err));
      }
      if (abort.signal.aborted) return;
      retryTimer = setTimeout(() => void subscribe(), delay);
      delay = Math.min(delay * 2, LONGEST_RETRY_MS);
    };
    void subscribe();

    return () => {
      abort.abort();
      clearTimeout(retryTimer);
    };
  }, [client, sessionToken, projectId, sessionId]);

  return { view, error };
}
