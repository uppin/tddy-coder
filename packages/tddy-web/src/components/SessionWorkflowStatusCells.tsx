import type { SessionEntry } from "../gen/connection_pb";
import { TableCell } from "@/components/ui/table";

function displayOrDash(value: string | undefined): string {
  const t = value?.trim();
  return t ? t : "—";
}

/** Renders TUI-parity workflow columns for a session row (from `ListSessions` extended fields). */
export function SessionWorkflowStatusCells({ session }: { session: SessionEntry }) {
  const sessionId = session.sessionId;
  const goal = displayOrDash(session.workflowGoal);
  const state = displayOrDash(session.workflowState);
  const elapsed = displayOrDash(session.elapsedDisplay);
  const agent = displayOrDash(session.agent);
  const model = displayOrDash(session.model);

  return (
    <>
      <TableCell data-session-col="goal" data-testid={`session-row-workflow-goal-${sessionId}`}>
        {goal}
      </TableCell>
      <TableCell data-session-col="workflow" data-testid={`session-row-workflow-state-${sessionId}`}>
        {state}
      </TableCell>
      <TableCell data-session-col="elapsed" data-testid={`session-row-elapsed-${sessionId}`}>
        {elapsed}
      </TableCell>
      <TableCell data-session-col="agent" data-testid={`session-row-agent-${sessionId}`}>
        {agent}
      </TableCell>
      <TableCell data-session-col="model" data-testid={`session-row-model-${sessionId}`}>
        {model}
      </TableCell>
    </>
  );
}
