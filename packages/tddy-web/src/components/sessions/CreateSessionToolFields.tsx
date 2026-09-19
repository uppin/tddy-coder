import type { ReactNode } from "react";
import type { SessionEntry } from "../../gen/session_pb";
import type { HostReadFailure } from "../../rpc/useHostFanOut";
import { CreateSessionAgentSelect } from "./CreateSessionAgentSelect";
import { inputClass, labelClass } from "./createSessionFormStyles";
import { WORKFLOW_RECIPES } from "./createSessionRecipes";
import type { SelectableAgent } from "./selectableAgentOptions";

export interface CreateSessionToolFieldsProps {
  offeredAgents: readonly SelectableAgent[];
  offeredHostFailures: readonly HostReadFailure[];
  hostsAdvertised: boolean;
  selectedAgentValue: string;
  setAgent: (agent: string) => void;
  setDaemonInstanceId: (daemonInstanceId: string) => void;
  recipe: string;
  setRecipe: (recipe: string) => void;
  prStackBaseSessionId: string;
  setPrStackBaseSessionId: (sessionId: string) => void;
  stackBaseSessionOptions: SessionEntry[];
  modelField: ReactNode;
}

/** Tool session fields. */
export function CreateSessionToolFields({
  offeredAgents,
  offeredHostFailures,
  hostsAdvertised,
  selectedAgentValue,
  setAgent,
  setDaemonInstanceId,
  recipe,
  setRecipe,
  prStackBaseSessionId,
  setPrStackBaseSessionId,
  stackBaseSessionOptions,
  modelField,
}: CreateSessionToolFieldsProps) {
  return (
    <>
      <CreateSessionAgentSelect
        agents={offeredAgents}
        failures={offeredHostFailures}
        hostsAdvertised={hostsAdvertised}
        selectedValue={selectedAgentValue}
        onPick={(picked) => {
          setAgent(picked.id);
          // The session runs where its agent is resolvable, so picking one names its host.
          setDaemonInstanceId(picked.daemonInstanceId);
        }}
      />

      <div>
        <label className={labelClass} htmlFor="create-session-recipe">
          Recipe
        </label>
        <select
          id="create-session-recipe"
          data-testid="create-session-recipe-select"
          className={inputClass}
          value={recipe}
          onChange={(e) => setRecipe(e.target.value)}
        >
          {WORKFLOW_RECIPES.map((r) => (
            <option key={r} value={r}>
              {r}
            </option>
          ))}
        </select>
      </div>

      {/* Base the stack on — seeds the new orchestrator's stack with one existing session's
          branch as its single root node, instead of leaving the agent to plan a stack it cannot
          know about. Hangs off the recipe rather than the branch mode: an orchestrator has no
          branch of its own, so there is no branch mode for the control to qualify. */}
      {recipe === "pr-stack" && (
        <div>
          <label className={labelClass} htmlFor="create-session-pr-stack-base-session">
            Base the stack on
          </label>
          <select
            id="create-session-pr-stack-base-session"
            data-testid="create-session-pr-stack-base-session-select"
            className={inputClass}
            value={prStackBaseSessionId}
            onChange={(e) => setPrStackBaseSessionId(e.target.value)}
          >
            <option value="">None (agent plans the stack)</option>
            {/* Labelled by the branch as well as the id: the branch is what the seeded node is
                bound to, and what every descendant is based on. */}
            {stackBaseSessionOptions.map((s) => (
              <option key={s.sessionId} value={s.sessionId}>
                {`${s.sessionId} — ${s.branch}`}
              </option>
            ))}
          </select>
        </div>
      )}

      {modelField}
    </>
  );
}
