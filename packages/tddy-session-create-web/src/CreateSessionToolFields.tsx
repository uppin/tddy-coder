import type { ReactNode } from "react";
import { InlineError } from "tddy-ui-web";
import { CreateSessionAgentSelect } from "./CreateSessionAgentSelect";
import { inputClass, labelClass } from "./createSessionFormStyles";
import { WORKFLOW_RECIPES } from "./createSessionRecipes";
import type { SelectableAgent } from "./selectableAgentOptions";
import { type HostReadFailure } from "tddy-client-web";

export interface CreateSessionToolFieldsProps {
  offeredAgents: readonly SelectableAgent[];
  offeredHostFailures: readonly HostReadFailure[];
  hostsAdvertised: boolean;
  selectedAgentValue: string;
  setAgent: (agent: string) => void;
  setDaemonInstanceId: (daemonInstanceId: string) => void;
  recipe: string;
  setRecipe: (recipe: string) => void;
  modelField: ReactNode;
  /** Why the tool catalog could not be read — a tool session cannot start without a tool, and the
   *  form has none to pick when this fails. */
  toolsError?: string | null;
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
  modelField,
  toolsError = null,
}: CreateSessionToolFieldsProps) {
  return (
    <>
      <InlineError
        testId="create-session-tools-load-error"
        message={toolsError && `Tools unavailable: ${toolsError}`}
      />

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

      {modelField}
    </>
  );
}
