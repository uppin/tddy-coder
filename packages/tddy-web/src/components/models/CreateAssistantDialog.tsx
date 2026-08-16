import { useState } from "react";
import type { AssignableTool } from "../../gen/models_pb";
import type { ModelRow } from "../../utils/mergeRegistryEntries";

/**
 * Compose a model, a system prompt and a set of tools into an assistant — a `--agent <name>` the
 * daemon can then run. The tool choices are the **daemon's** exec catalog as advertised by
 * `ListAssignableTools`; the web offers no tool list of its own, so a daemon that gains a tool
 * offers it here without a web release
 * (docs/ft/web/1-WIP/PRD-2026-08-16-models-and-assistants.md § AC8).
 */

const fieldClassName =
  "rounded border border-input bg-background px-2 py-1 text-sm text-foreground";

export interface CreateAssistantDialogProps {
  model: ModelRow;
  tools: readonly AssignableTool[];
  /** Resolves to the error to show, or `""` once the assistant exists on the owning daemon. */
  onSubmit: (input: {
    name: string;
    label: string;
    systemPrompt: string;
    tools: string[];
  }) => Promise<string>;
  onClose: () => void;
}

export function CreateAssistantDialog({
  model,
  tools,
  onSubmit,
  onClose,
}: CreateAssistantDialogProps) {
  const [name, setName] = useState("");
  const [label, setLabel] = useState("");
  const [systemPrompt, setSystemPrompt] = useState("");
  const [selectedTools, setSelectedTools] = useState<string[]>([]);
  const [error, setError] = useState("");

  const toggleTool = (toolName: string, checked: boolean) =>
    setSelectedTools((current) =>
      checked ? [...current, toolName] : current.filter((t) => t !== toolName),
    );

  const submit = async () => {
    // Send the tools in the daemon's catalog order, so the stored set does not depend on the order
    // the operator happened to tick the boxes in.
    const orderedTools = tools.map((t) => t.name).filter((t) => selectedTools.includes(t));
    const failure = await onSubmit({
      name: name.trim(),
      label: label.trim(),
      systemPrompt: systemPrompt.trim(),
      tools: orderedTools,
    });
    setError(failure);
    if (failure === "") onClose();
  };

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-background/80 p-4">
      <div
        data-testid="models-create-assistant-dialog"
        role="dialog"
        aria-label="Create assistant"
        className="flex max-h-full w-full max-w-lg flex-col gap-2 overflow-auto rounded-md border border-border bg-background p-4 text-sm text-foreground"
      >
        <h2 className="text-sm font-semibold">Create assistant</h2>
        <div className="text-xs text-muted-foreground">
          {model.label} · {model.modelId} · {model.daemonInstanceId}
        </div>
        <input
          data-testid="models-create-assistant-name"
          placeholder="Agent name (the --agent value)"
          className={fieldClassName}
          value={name}
          onChange={(e) => setName(e.target.value)}
        />
        <input
          data-testid="models-create-assistant-label"
          placeholder="Label"
          className={fieldClassName}
          value={label}
          onChange={(e) => setLabel(e.target.value)}
        />
        <textarea
          data-testid="models-create-assistant-system-prompt"
          placeholder="System prompt"
          rows={4}
          className={fieldClassName}
          value={systemPrompt}
          onChange={(e) => setSystemPrompt(e.target.value)}
        />
        <fieldset className="flex flex-col gap-1">
          <legend className="text-xs text-muted-foreground">Tools</legend>
          {tools.map((tool) => (
            <label key={tool.name} className="flex items-center gap-2 text-xs">
              <input
                type="checkbox"
                data-testid={`models-create-assistant-tool-${tool.name}`}
                checked={selectedTools.includes(tool.name)}
                onChange={(e) => toggleTool(tool.name, e.target.checked)}
              />
              <span>{tool.name}</span>
              {tool.isMutating ? <span className="text-muted-foreground">(mutating)</span> : null}
            </label>
          ))}
        </fieldset>
        {error ? <div className="text-xs text-destructive">{error}</div> : null}
        <div className="flex items-center gap-2">
          <button
            type="button"
            data-testid="models-create-assistant-submit"
            className="rounded-md border border-input px-3 py-1 text-sm font-medium text-foreground hover:bg-accent"
            onClick={() => void submit()}
          >
            Create
          </button>
          <button
            type="button"
            data-testid="models-create-assistant-cancel"
            className="rounded-md border border-input px-3 py-1 text-sm font-medium text-foreground hover:bg-accent"
            onClick={onClose}
          >
            Cancel
          </button>
        </div>
      </div>
    </div>
  );
}
