import { AgentKind } from "../../gen/types_pb";

export interface AgentKindBadgeProps {
  /** What runs the agent's turns, as the catalog row or roster entry says. */
  kind: AgentKind;
  /** The harness of a coding agent (`claude-acp`, `codex-acp`, `opencode`); empty for a model. */
  harness: string;
  testId: string;
}

/**
 * "coding agent · <harness>" on a row whose agent is a coding agent the daemon launches; nothing on
 * a model (definition or registry assistant) row, which is what every row was before coding agents.
 *
 * PRD: docs/ft/daemon/1-WIP/PRD-2026-10-24-coagents-acp-subagent.md
 */
export function AgentKindBadge({ kind, harness, testId }: AgentKindBadgeProps) {
  if (kind !== AgentKind.CODING_AGENT) return null;
  return (
    <span
      data-testid={testId}
      className="rounded border border-border px-1 text-[10px] text-muted-foreground"
    >
      {`coding agent · ${harness}`}
    </span>
  );
}
