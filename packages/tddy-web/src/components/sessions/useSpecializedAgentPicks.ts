import { useState, type Dispatch, type SetStateAction } from "react";
import type { AgentPlacementChoice } from "./agentPlacementChoice";

export interface SpecializedAgentPicks {
  /** The specialized agents picked to attach at start, as qualified ids. */
  selectedAgentIds: string[];
  /** The subset of the picked agents whose conversations each work on a clone of their own. */
  clonePlacedAgentIds: string[];
  /** Replaces the picks wholesale — the claude-cli fields seed them from a recipe. */
  setSelectedAgentIds: Dispatch<SetStateAction<string[]>>;
  toggleAgent: (agentId: string) => void;
  setAgentPlacement: (agentId: string, placement: AgentPlacementChoice) => void;
}

/**
 * The create form's specialized-agent picks and their placements. Picks are qualified ids rather
 * than bare names because the picker lists every host's agents and two hosts routinely offer a def
 * of the same name — a bare name cannot say which of them was picked. The clone placements are kept
 * beside the picks rather than inside them so the picks stay what the list of ids means on its own;
 * every other picked agent works on the session's worktree.
 */
export function useSpecializedAgentPicks(): SpecializedAgentPicks {
  const [selectedAgentIds, setSelectedAgentIds] = useState<string[]>([]);
  const [clonePlacedAgentIds, setClonePlacedAgentIds] = useState<string[]>([]);

  const toggleAgent = (agentId: string) => {
    setSelectedAgentIds((prev) =>
      prev.includes(agentId) ? prev.filter((id) => id !== agentId) : [...prev, agentId],
    );
  };

  const setAgentPlacement = (agentId: string, placement: AgentPlacementChoice) => {
    setClonePlacedAgentIds((prev) => {
      const others = prev.filter((id) => id !== agentId);
      return placement === "clone" ? [...others, agentId] : others;
    });
  };

  return {
    selectedAgentIds,
    clonePlacedAgentIds,
    setSelectedAgentIds,
    toggleAgent,
    setAgentPlacement,
  };
}
