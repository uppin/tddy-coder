import { useState, type ReactNode } from "react";
import { CreateSessionEffortField } from "./CreateSessionEffortField";
import type { SelectableAgent } from "./selectableAgentOptions";

/**
 * The effort the New-session form starts the selected agent at (`#orchestrators` 3/11), and its
 * picker. The levels and the preselected one are what the daemon served for the agent; until the
 * operator chooses, the agent's served default stands, and a choice the agent does not offer (left
 * from another agent's picker) reads as the newly selected agent's served default.
 */
export function useCreateSessionEffort(agent: SelectableAgent | undefined): {
  effort: string;
  effortField: ReactNode;
} {
  const [chosen, setChosen] = useState<string | null>(null);
  const levels = agent?.effortLevels ?? [];
  const served = agent?.defaultEffort ?? "";
  const effort =
    chosen === null ? served : chosen === "" || levels.includes(chosen) ? chosen : served;
  return {
    effort,
    effortField: <CreateSessionEffortField levels={levels} effort={effort} setEffort={setChosen} />,
  };
}
