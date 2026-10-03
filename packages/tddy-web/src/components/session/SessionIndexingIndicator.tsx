import type { Client } from "@connectrpc/connect";
import type { CodeNavigationService } from "../../gen/code_navigation_pb";

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
 * Follows `code_navigation.WatchCodeIndex(session)`; never blocks the session.
 *
 * PRD: docs/ft/web/1-WIP/PRD-2026-10-03-indexing-indicators.md
 */
export function SessionIndexingIndicator(_props: SessionIndexingIndicatorProps) {
  // TODO(indexing-indicators): follow `client.watchCodeIndex({ sessionToken, sessionId })`; render
  // `data-testid="session-indexing-indicator"` with "Indexing — <phase> <n>%" until `ready`, the
  // `error` when the warm fails, and nothing after `ready` or for a stream that ends empty.
  return null;
}
