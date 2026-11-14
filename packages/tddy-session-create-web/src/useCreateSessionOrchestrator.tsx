import { useState, type ReactNode } from "react";
import { CreateSessionOrchestratorField } from "./CreateSessionOrchestratorField";
import { hasCapability } from "./codingAgentMetadata";
import type { CreateSessionProjectHome } from "./createSessionPaneProps";

type SessionAgent = Parameters<typeof hasCapability>[0];

/**
 * The **Session orchestrator** switch's state and element (`#orchestrators` 1/11). The switch is
 * offered only on the ordinary form, for an agent whose spec declares `project_home_main` — the
 * agents that can be held read-only — so `asOrchestrator` is never true for any other agent, even
 * when the switch was left on before the agent changed. `orchestratorField` renders nothing when
 * the switch is not offered.
 */
export function useCreateSessionOrchestrator(
  sessionAgent: SessionAgent,
  projectHome: CreateSessionProjectHome | undefined,
): { asOrchestrator: boolean; orchestratorField: ReactNode } {
  const [orchestrator, setOrchestrator] = useState(false);
  const offered = projectHome === undefined && hasCapability(sessionAgent, "project_home_main");
  return {
    asOrchestrator: offered && orchestrator,
    orchestratorField: offered ? (
      <CreateSessionOrchestratorField orchestrator={orchestrator} setOrchestrator={setOrchestrator} />
    ) : null,
  };
}
