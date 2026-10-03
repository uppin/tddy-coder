import { useEffect, useState } from "react";
import type { Client } from "@connectrpc/connect";
import type { CodeIndexProgress, CodeNavigationService } from "../../gen/code_navigation_pb";

export interface SessionIndexingIndicatorProps {
  /** `code_navigation.CodeNavigationService` on the host that owns the session. */
  client: Client<typeof CodeNavigationService>;
  sessionToken: string;
  sessionId: string;
}

/** What the indicator shows: the latest progress, or the failure that ended the stream. */
type IndexState = { kind: "progress"; progress: CodeIndexProgress } | { kind: "failed"; reason: string };

/**
 * The session header's code-index indicator: "Indexing — <phase> <n>%" while the session's code
 * index warms, the failure's reason if the warm fails, and nothing once the index is ready — or when
 * nothing is warming it (no `index_daemon:` on the host), so a session without a warm index shows no
 * indicator at all.
 *
 * Follows `code_navigation.WatchCodeIndex(session)`; never blocks the session.
 *
 * PRD: docs/ft/web/1-WIP/PRD-2026-10-03-indexing-indicators.md
 */
export function SessionIndexingIndicator({
  client,
  sessionToken,
  sessionId,
}: SessionIndexingIndicatorProps) {
  const [state, setState] = useState<IndexState | null>(null);

  useEffect(() => {
    // Consumed with a `cancelled` flag rather than an `AbortSignal`: the LiveKit transport accepts a
    // signal for server-streaming calls and never reads it.
    let cancelled = false;
    setState(null);
    (async () => {
      try {
        for await (const progress of client.watchCodeIndex({ sessionToken, sessionId })) {
          if (cancelled) return;
          if (progress.ready) {
            setState(null);
          } else if (progress.error !== "") {
            setState({ kind: "failed", reason: progress.error });
          } else {
            setState({ kind: "progress", progress });
          }
        }
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

  if (state === null) return null;
  const text =
    state.kind === "failed"
      ? `Indexing failed — ${state.reason}`
      : `Indexing — ${state.progress.phase} ${state.progress.percentage}%`;
  return (
    <span data-testid="session-indexing-indicator" className="mr-auto text-xs text-muted-foreground">
      {text}
    </span>
  );
}
