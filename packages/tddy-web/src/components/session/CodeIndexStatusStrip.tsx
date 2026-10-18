import type { CodeIndexProgress } from "../../gen/code_navigation_pb";
import {
  currentPhaseHeadline,
  phaseHeadline,
  type CodeIndexStatus,
} from "./useCodeIndexProgress";

export interface CodeIndexStatusStripProps {
  /** Where the worktree's code index stands, as the pane's `CodeSession` reports it. */
  status: CodeIndexStatus;
}

/**
 * The Code pane's bottom strip: where the worktree's code index stands, so a definition or hover
 * can be read against it. Unlike the header's indicator it stays visible once the index is ready,
 * and while loading it adds the message of the phase it names (the crate being indexed); when
 * several phases run at once it counts the others, listing every one in its tooltip.
 */
export function CodeIndexStatusStrip({ status }: CodeIndexStatusStripProps) {
  const { summary, detail } = describe(status);
  const others = status.kind === "progress" ? otherPhases(status.progress) : null;
  return (
    <div
      data-testid="worktree-code-index-status"
      data-state={status.kind}
      role="status"
      className={`flex shrink-0 items-baseline gap-2 border-t border-border px-3 py-1 text-xs ${
        status.kind === "failed" ? "text-destructive" : "text-muted-foreground"
      }`}
    >
      <span data-testid="worktree-code-index-status-summary" className="shrink-0">
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
      {others !== null && (
        <span
          data-testid="worktree-code-index-status-more"
          className="shrink-0 underline decoration-dotted"
          title={others.tooltip}
        >
          {`+${others.count} more`}
        </span>
      )}
    </div>
  );
}

function describe(status: CodeIndexStatus): { summary: string; detail: string } {
  switch (status.kind) {
    case "waiting":
      return { summary: "Code index: checking…", detail: "" };
    case "idle":
      return { summary: "Code index not loaded", detail: "" };
    case "progress":
      return {
        summary: `Indexing — ${currentPhaseHeadline(status.progress)}`,
        detail: currentPhaseMessage(status.progress),
      };
    case "ready":
      return { summary: "Code index ready", detail: "" };
    case "failed":
      return { summary: `Indexing failed — ${status.reason}`, detail: "" };
  }
}

/** The message of the phase the headline names — the latest-begun phase of that title. */
function currentPhaseMessage(progress: CodeIndexProgress): string {
  const current = progress.phases.filter((phase) => phase.title === progress.phase).at(-1);
  return current?.message ?? "";
}

/** How many phases run besides the headline's, and a tooltip listing every running phase. */
function otherPhases(progress: CodeIndexProgress): { count: number; tooltip: string } | null {
  if (progress.phases.length < 2) return null;
  const tooltip = progress.phases
    .map((phase) =>
      phase.message === "" ? phaseHeadline(phase) : `${phaseHeadline(phase)} — ${phase.message}`,
    )
    .join("\n");
  return { count: progress.phases.length - 1, tooltip };
}
