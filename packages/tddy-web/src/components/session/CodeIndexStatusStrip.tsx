import type { Client } from "@connectrpc/connect";
import type { CodeNavigationService } from "../../gen/code_navigation_pb";
import {
  useCodeIndexProgress,
  type CodeIndexState,
} from "./useCodeIndexProgress";

export interface CodeIndexStatusStripProps {
  /** `code_navigation.CodeNavigationService` on the host that owns the session. */
  client: Client<typeof CodeNavigationService>;
  sessionToken: string;
  sessionId: string;
}

/**
 * The Code pane's bottom strip: where the session's code index stands, so a definition or hover
 * that finds nothing can be read against it. Unlike the header's indicator it stays visible once the
 * index is ready, and while loading it adds the server's own line about what it is doing (the crate
 * being checked or indexed) — the phases that take minutes report little else.
 *
 * Follows `code_navigation.WatchCodeIndex(session)` through `useCodeIndexProgress`.
 */
export function CodeIndexStatusStrip({
  client,
  sessionToken,
  sessionId,
}: CodeIndexStatusStripProps) {
  const state = useCodeIndexProgress(client, sessionToken, sessionId);
  const { summary, detail } = describe(state);
  return (
    <div
      data-testid="worktree-code-index-status"
      data-state={state.kind}
      role="status"
      className={`flex shrink-0 items-baseline gap-2 border-t border-border px-3 py-1 text-xs ${
        state.kind === "failed" ? "text-destructive" : "text-muted-foreground"
      }`}
    >
      <span
        data-testid="worktree-code-index-status-summary"
        className="shrink-0"
      >
        {summary}
      </span>
      {detail !== "" && (
        <span
          data-testid="worktree-code-index-status-detail"
          className="min-w-0 truncate"
          title={detail}
        >
          {detail}
        </span>
      )}
    </div>
  );
}

function describe(state: CodeIndexState): { summary: string; detail: string } {
  switch (state.kind) {
    case "waiting":
      return { summary: "Code index: checking…", detail: "" };
    case "progress":
      return {
        summary: `Indexing — ${state.progress.phase} ${state.progress.percentage}%`,
        detail: state.progress.line,
      };
    case "ready":
      return { summary: "Code index ready", detail: "" };
    case "failed":
      return { summary: `Indexing failed — ${state.reason}`, detail: "" };
    case "untracked":
      return {
        summary: "Code index not warmed for this session",
        detail:
          "it loads on the first definition or hover, which may answer nothing until then",
      };
  }
}
