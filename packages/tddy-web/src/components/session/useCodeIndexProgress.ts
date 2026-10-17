import { useEffect, useState } from "react";
import type { Client } from "@connectrpc/connect";
import type { CodeIndexProgress, CodeNavigationService } from "../../gen/code_navigation_pb";

/**
 * Where a session's code-index warm-up stands, as `WatchCodeIndex(session)` reports it.
 *
 * - `waiting` — nothing heard yet.
 * - `progress` — the latest progress message: the phase running now, its percentage, and the
 *   server's own line about what it is doing.
 * - `ready` — the warm finished: the index is loaded and queryable.
 * - `failed` — the warm, or the call following it, failed; the reason.
 * - `untracked` — the stream ended without a message: nothing on the host is warming this session
 *   (no `index_daemon:`, a worktree that is not a Cargo workspace, or a warm the host no longer
 *   remembers), so the index loads on the first navigation request instead.
 */
export type CodeIndexState =
  | { kind: "waiting" }
  | { kind: "progress"; progress: CodeIndexProgress }
  | { kind: "ready" }
  | { kind: "failed"; reason: string }
  | { kind: "untracked" };

/** Follow `code_navigation.WatchCodeIndex(session)` on `client` until it ends or the deps change. */
export function useCodeIndexProgress(
  client: Client<typeof CodeNavigationService>,
  sessionToken: string,
  sessionId: string,
): CodeIndexState {
  const [state, setState] = useState<CodeIndexState>({ kind: "waiting" });

  useEffect(() => {
    // Consumed with a `cancelled` flag rather than an `AbortSignal`: the LiveKit transport accepts a
    // signal for server-streaming calls and never reads it.
    let cancelled = false;
    let heard = false;
    setState({ kind: "waiting" });
    (async () => {
      try {
        for await (const progress of client.watchCodeIndex({ sessionToken, sessionId })) {
          if (cancelled) return;
          heard = true;
          if (progress.ready) {
            setState({ kind: "ready" });
          } else if (progress.error !== "") {
            setState({ kind: "failed", reason: progress.error });
          } else {
            setState({ kind: "progress", progress });
          }
        }
        if (!cancelled && !heard) setState({ kind: "untracked" });
      } catch (err) {
        if (!cancelled) {
          setState({ kind: "failed", reason: err instanceof Error ? err.message : String(err) });
        }
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [client, sessionToken, sessionId]);

  return state;
}
