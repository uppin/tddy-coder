import { AgentPlacement } from "../../gen/session_agents_pb";

/**
 * Where a picked agent's conversations work: `shared` is the session's own worktree, `clone` a
 * worktree and branch of each conversation's own. The pickers speak these two words; the wire speaks
 * `session_agents.AgentPlacement`.
 */
export type AgentPlacementChoice = "shared" | "clone";

/** The wire placement a picker choice stands for. */
export function agentPlacementWire(choice: AgentPlacementChoice): AgentPlacement {
  return choice === "clone" ? AgentPlacement.CLONE : AgentPlacement.SHARED;
}

/** The picker choice a roster entry's wire placement reads as. */
export function agentPlacementChoiceOf(placement: AgentPlacement): AgentPlacementChoice {
  return placement === AgentPlacement.CLONE ? "clone" : "shared";
}
