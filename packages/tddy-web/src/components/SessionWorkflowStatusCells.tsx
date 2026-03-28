import { useEffect } from "react";
import type { SessionEntry } from "../gen/connection_pb";

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

  useEffect(() => {
    if (import.meta.env.DEV) {
      console.debug("[SessionWorkflowStatusCells]", sessionId, {
        workflowGoal: goal,
        workflowState: state,
        elapsedDisplay: elapsed,
        agent,
        model,
      });
    }
  }, [sessionId, goal, state, elapsed, agent, model]);

  return (
    <>
      <td style={{ padding: 8 }} data-testid={`session-row-workflow-goal-${sessionId}`}>
        {goal}
      </td>
      <td style={{ padding: 8 }} data-testid={`session-row-workflow-state-${sessionId}`}>
        {state}
      </td>
      <td style={{ padding: 8 }} data-testid={`session-row-elapsed-${sessionId}`}>
        {elapsed}
      </td>
      <td style={{ padding: 8 }} data-testid={`session-row-agent-${sessionId}`}>
        {agent}
      </td>
      <td style={{ padding: 8 }} data-testid={`session-row-model-${sessionId}`}>
        {model}
      </td>
    </>
  );
}
