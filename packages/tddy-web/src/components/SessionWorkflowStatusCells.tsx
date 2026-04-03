import type { SessionEntry } from "../gen/connection_pb";
import { TableCell } from "@/components/ui/table";
import type { SessionTableColumnKey } from "./connection/sessionTableColumns";

function displayOrDash(value: string | undefined): string {
  const t = value?.trim();
  return t ? t : "—";
}

const WORKFLOW_COLUMN_KEYS = ["goal", "workflow", "elapsed", "agent", "model"] as const;

/** Renders TUI-parity workflow columns for a session row (from `ListSessions` extended fields). */
export function SessionWorkflowStatusCells({
  session,
  visibleColumnKeys,
}: {
  session: SessionEntry;
  visibleColumnKeys: ReadonlySet<SessionTableColumnKey>;
}) {
  const sessionId = session.sessionId;
  const goal = displayOrDash(session.workflowGoal);
  const state = displayOrDash(session.workflowState);
  const elapsed = displayOrDash(session.elapsedDisplay);
  const agent = displayOrDash(session.agent);
  const model = displayOrDash(session.model);

  const hideStyle = (key: (typeof WORKFLOW_COLUMN_KEYS)[number]) =>
    visibleColumnKeys.has(key) ? undefined : { display: "none" as const };

  return (
    <>
      <TableCell
        style={hideStyle("goal")}
        data-testid={`session-row-workflow-goal-${sessionId}`}
      >
        {goal}
      </TableCell>
      <TableCell
        style={hideStyle("workflow")}
        data-testid={`session-row-workflow-state-${sessionId}`}
      >
        {state}
      </TableCell>
      <TableCell
        style={hideStyle("elapsed")}
        data-testid={`session-row-elapsed-${sessionId}`}
      >
        {elapsed}
      </TableCell>
      <TableCell
        style={hideStyle("agent")}
        data-testid={`session-row-agent-${sessionId}`}
      >
        {agent}
      </TableCell>
      <TableCell
        style={hideStyle("model")}
        data-testid={`session-row-model-${sessionId}`}
      >
        {model}
      </TableCell>
    </>
  );
}
