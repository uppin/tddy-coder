import type { AgentModelsState } from "../../rpc/useAgentModels";
import { inputClass, labelClass } from "./createSessionFormStyles";

export interface CreateSessionModelFieldProps {
  agentModels: AgentModelsState;
  model: string;
  setModel: (model: string) => void;
}

/**
 * Model selector — shared by both session types, populated from the daemon-advertised catalog for
 * the current backend. While the probe is in flight it shows a loading line; a failed probe shows
 * an inline error and renders no select (so `model` stays empty and Create is disabled).
 */
export function CreateSessionModelField({
  agentModels,
  model,
  setModel,
}: CreateSessionModelFieldProps) {
  return (
    <div>
      <label className={labelClass} htmlFor="create-session-model">
        Model
      </label>
      {agentModels.loading ? (
        <p data-testid="create-session-model-loading" className="text-sm text-muted-foreground">
          Loading models…
        </p>
      ) : agentModels.error !== null ? (
        <p data-testid="create-session-model-error" className="text-sm text-destructive">
          {agentModels.error}
        </p>
      ) : (
        <select
          id="create-session-model"
          data-testid="create-session-model-select"
          className={inputClass}
          value={model}
          onChange={(e) => setModel(e.target.value)}
        >
          {agentModels.models.map((m) => (
            <option key={m.id} value={m.id}>
              {m.label}
            </option>
          ))}
        </select>
      )}
    </div>
  );
}
