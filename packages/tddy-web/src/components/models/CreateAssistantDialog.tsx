import { useState } from "react";
import type { ModelRow } from "../../utils/mergeRegistryEntries";
import { AssistantToolPicker } from "./AssistantToolPicker";
import { ModelsDialogShell } from "./ModelsDialogShell";
import type { ToolCatalog } from "./useModelRegistryFanOut";

/**
 * Compose a model, a system prompt and a set of tools into an assistant — a `--agent <name>` the
 * daemon can then run. The tool choices are the **daemon's** exec catalog as advertised by
 * `ListAssignableTools`; the web offers no tool list of its own, so a daemon that gains a tool
 * offers it here without a web release
 * (docs/ft/web/1-WIP/PRD-2026-08-16-models-and-assistants.md § AC8).
 *
 * Creating is refused until that catalog is known: an assistant is persisted with the tools it was
 * created with, so one composed from a catalog that never arrived is a permanently toolless agent
 * that nothing on screen distinguishes from a deliberately toolless one.
 */

const fieldClassName =
  "rounded border border-input bg-background px-2 py-1 text-sm text-foreground";

export interface CreateAssistantDialogProps {
  model: ModelRow;
  /** The owning daemon's exec catalog, or why it is not known. */
  toolCatalog: ToolCatalog;
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
  toolCatalog,
  onSubmit,
  onClose,
}: CreateAssistantDialogProps) {
  const [name, setName] = useState("");
  const [label, setLabel] = useState("");
  const [systemPrompt, setSystemPrompt] = useState("");
  const [selectedTools, setSelectedTools] = useState<string[]>([]);
  const [error, setError] = useState("");
  // A second click while the create is in flight mints a second assistant on the daemon, which the
  // operator then has to find and delete.
  const [submitting, setSubmitting] = useState(false);

  const toggleTool = (toolName: string, checked: boolean) =>
    setSelectedTools((current) =>
      checked ? [...current, toolName] : current.filter((t) => t !== toolName),
    );

  const submit = async () => {
    if (submitting || toolCatalog.status !== "ready") return;
    // Send the tools in the daemon's catalog order, so the stored set does not depend on the order
    // the operator happened to tick the boxes in.
    const orderedTools = toolCatalog.tools
      .map((t) => t.name)
      .filter((t) => selectedTools.includes(t));
    setSubmitting(true);
    try {
      const failure = await onSubmit({
        name: name.trim(),
        label: label.trim(),
        systemPrompt: systemPrompt.trim(),
        tools: orderedTools,
      });
      setError(failure);
      if (failure === "") onClose();
    } finally {
      setSubmitting(false);
    }
  };

  return (
    <ModelsDialogShell
      testId="models-create-assistant-dialog"
      label="Create assistant"
      className="flex max-h-full w-full max-w-lg flex-col gap-2 overflow-auto rounded-md border border-border bg-background p-4 text-sm text-foreground"
      onClose={onClose}
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
      <AssistantToolPicker
        idPrefix="models-create-assistant"
        catalog={toolCatalog}
        selected={selectedTools}
        onToggle={toggleTool}
      />
      {error ? (
        <div data-testid="models-create-assistant-error" className="text-xs text-destructive">
          {error}
        </div>
      ) : null}
      <div className="flex items-center gap-2">
        <button
          type="button"
          data-testid="models-create-assistant-submit"
          className="rounded-md border border-input px-3 py-1 text-sm font-medium text-foreground hover:bg-accent disabled:opacity-50"
          disabled={submitting || toolCatalog.status !== "ready"}
          onClick={() => void submit()}
        >
          {submitting ? "Creating…" : "Create"}
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
    </ModelsDialogShell>
  );
}
