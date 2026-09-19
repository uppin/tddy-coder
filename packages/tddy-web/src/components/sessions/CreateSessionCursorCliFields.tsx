import type { ReactNode } from "react";
import { inputClass, labelClass } from "./createSessionFormStyles";
import { WORKFLOW_RECIPES } from "./createSessionRecipes";

export interface CreateSessionCursorCliFieldsProps {
  modelField: ReactNode;
  agentPickerSection: ReactNode;
  sandbox: boolean;
  setSandbox: (sandbox: boolean) => void;
  initialPrompt: string;
  setInitialPrompt: (initialPrompt: string) => void;
  managedCodebase: boolean;
  setManagedCodebase: (managedCodebase: boolean) => void;
  setSemanticIndex: (semanticIndex: boolean) => void;
  setSelectedAgentIds: (agentIds: string[]) => void;
  recipe: string;
  setRecipe: (recipe: string) => void;
  semanticIndex: boolean;
}

/** Cursor CLI session fields. */
export function CreateSessionCursorCliFields({
  modelField,
  agentPickerSection,
  sandbox,
  setSandbox,
  initialPrompt,
  setInitialPrompt,
  managedCodebase,
  setManagedCodebase,
  setSemanticIndex,
  setSelectedAgentIds,
  recipe,
  setRecipe,
  semanticIndex,
}: CreateSessionCursorCliFieldsProps) {
  return (
    <>
      {modelField}
      <div>
        <label className="flex items-center gap-2 text-sm text-muted-foreground">
          <input
            data-testid="create-session-sandbox-toggle"
            type="checkbox"
            className="h-4 w-4 rounded border-input"
            checked={sandbox}
            onChange={(e) => setSandbox(e.target.checked)}
          />
          Sandbox
        </label>
      </div>
      <div>
        <label className={labelClass} htmlFor="create-session-initial-prompt">
          Initial prompt
        </label>
        <textarea
          id="create-session-initial-prompt"
          data-testid="create-session-initial-prompt-input"
          className={`${inputClass} resize-y`}
          rows={3}
          value={initialPrompt}
          onChange={(e) => setInitialPrompt(e.target.value)}
          placeholder="Optional initial prompt"
        />
      </div>
      <div>
        <label className="flex items-center gap-2 text-sm text-muted-foreground">
          <input
            data-testid="create-session-managed-codebase-toggle"
            type="checkbox"
            className="h-4 w-4 rounded border-input"
            checked={managedCodebase}
            onChange={(e) => {
              setManagedCodebase(e.target.checked);
              // Closing the section clears what only it could offer, rather than leaving the
              // values to be stripped at submit: a selection the operator can no longer see is
              // one the form must no longer hold, and a request that disagrees with the screen
              // is how a picked agent went missing without an error.
              if (!e.target.checked) {
                setSemanticIndex(false);
                setSelectedAgentIds([]);
              }
            }}
          />
          Managed codebase
        </label>
        {managedCodebase && (
          <div className="mt-2 space-y-3 pl-4">
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
            {agentPickerSection}
            <div>
              <label className="flex items-center gap-2 text-sm text-muted-foreground">
                <input
                  data-testid="create-session-semantic-index-toggle"
                  type="checkbox"
                  className="h-4 w-4 rounded border-input"
                  checked={semanticIndex}
                  onChange={(e) => setSemanticIndex(e.target.checked)}
                />
                Semantic index
              </label>
            </div>
          </div>
        )}
      </div>
    </>
  );
}
