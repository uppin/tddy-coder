import type { Client } from "@connectrpc/connect";
import type { CodeNavigationService } from "../../gen/code_navigation_pb";
import { currentPhaseHeadline, useCodeIndexProgress } from "./useCodeIndexProgress";

export interface SessionIndexingIndicatorProps {
  /** `code_navigation.CodeNavigationService` on the host that owns the session. */
  client: Client<typeof CodeNavigationService>;
  sessionToken: string;
  sessionId: string;
}

/**
 * The session header's code-index indicator: "Indexing — <phase> <n>%" while the session's
 * worktree index loads, the failure's reason if it fails, and nothing once the index is ready — or
 * while nothing holds it — so a session without a loading index shows no indicator at all.
 *
 * Follows `code_navigation.WatchCodeIndex(session)` (`useCodeIndexProgress`), which only watches:
 * it shows loads the Code pane or the agent's code tools started, and never starts one itself.
 * The Code pane's `CodeIndexStatusStrip` follows its own `CodeSession` in more detail.
 *
 * Feature docs: docs/ft/web/session-drawer.md (Start progress) and docs/ft/web/session-code-pane.md (Indexing indicator)
 */
export function SessionIndexingIndicator({
  client,
  sessionToken,
  sessionId,
}: SessionIndexingIndicatorProps) {
  const status = useCodeIndexProgress(client, sessionToken, sessionId);

  if (status.kind !== "progress" && status.kind !== "failed") return null;
  const text =
    status.kind === "failed"
      ? `Indexing failed — ${status.reason}`
      : `Indexing — ${currentPhaseHeadline(status.progress)}`;
  return (
    <span data-testid="session-indexing-indicator" className="mr-auto text-xs text-muted-foreground">
      {text}
    </span>
  );
}
