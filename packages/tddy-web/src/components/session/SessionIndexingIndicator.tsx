import type { Client } from "@connectrpc/connect";
import type { CodeNavigationService } from "../../gen/code_navigation_pb";
import { useCodeIndexProgress } from "./useCodeIndexProgress";

export interface SessionIndexingIndicatorProps {
  /** `code_navigation.CodeNavigationService` on the host that owns the session. */
  client: Client<typeof CodeNavigationService>;
  sessionToken: string;
  sessionId: string;
}

/**
 * The session header's code-index indicator: "Indexing — <phase> <n>%" while the session's code
 * index warms, the failure's reason if the warm fails, and nothing once the index is ready — or when
 * nothing is warming it (no `index_daemon:` on the host), so a session without a warm index shows no
 * indicator at all.
 *
 * Follows `code_navigation.WatchCodeIndex(session)` (`useCodeIndexProgress`); never blocks the
 * session. The Code pane's `CodeIndexStatusStrip` follows the same stream in more detail.
 *
 * Feature docs: docs/ft/web/session-drawer.md (Start progress) and docs/ft/web/session-code-pane.md (Indexing indicator)
 */
export function SessionIndexingIndicator({
  client,
  sessionToken,
  sessionId,
}: SessionIndexingIndicatorProps) {
  const state = useCodeIndexProgress(client, sessionToken, sessionId);

  if (state.kind !== "progress" && state.kind !== "failed") return null;
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
