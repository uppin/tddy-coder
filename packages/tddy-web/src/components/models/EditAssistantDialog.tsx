import { useState } from "react";
import { useAssistantToolSets } from "./useAssistantToolSets";
import type { AssistantRow } from "../../utils/mergeRegistryEntries";
import { AssistantToolPicker } from "./AssistantToolPicker";
import { ModelsDialogShell } from "./ModelsDialogShell";
import type { ToolCatalog } from "./useModelRegistryFanOut";

/**
 * Edit an assistant in place: its label, its system prompt, the tools it may call and the main-agent
 * tools it takes over — the four fields `UpdateAssistant` accepts. The `--agent` name and the model
 * it speaks as are its identity on the daemon and are shown, not offered for editing.
 *
 * Both tool sets open on what the daemon holds and are sent whole, so giving up a takeover is
 * expressible: an update that carried only what was ticked could never remove one.
 *
 * As in the create dialog, submitting is refused while the daemon's exec catalog is unknown: saving
 * then would replace the assistant's tool sets with whatever a failed read left behind.
 */

const fieldClassName =
  "rounded border border-input bg-background px-2 py-1 text-sm text-foreground";

export interface EditAssistantDialogProps {
  assistant: AssistantRow;
  /** The owning daemon's exec catalog, or why it is not known. */
  toolCatalog: ToolCatalog;
  /** Resolves to the error to show, or `""` once the daemon has stored the change. */
  onSubmit: (input: {
    label: string;
    systemPrompt: string;
    tools: string[];
    replaces: string[];
  }) => Promise<string>;
  onClose: () => void;
}

export function EditAssistantDialog({
  assistant,
  toolCatalog,
  onSubmit,
  onClose,
}: EditAssistantDialogProps) {
  const [label, setLabel] = useState(assistant.label);
  const [systemPrompt, setSystemPrompt] = useState(assistant.systemPrompt);
  const toolSets = useAssistantToolSets(assistant.tools, assistant.replaces);
  const [error, setError] = useState("");
  const [submitting, setSubmitting] = useState(false);

  const submit = async () => {
    if (submitting || toolCatalog.status !== "ready") return;
    const sent = toolSets.inCatalogOrder(toolCatalog.tools);
    setSubmitting(true);
    try {
      const failure = await onSubmit({
        label: label.trim(),
        systemPrompt: systemPrompt.trim(),
        tools: sent.tools,
        replaces: sent.replaces,
      });
      setError(failure);
      if (failure === "") onClose();
    } finally {
      setSubmitting(false);
    }
  };

  return (
    <ModelsDialogShell
      testId="models-edit-assistant-dialog"
      label={`Edit assistant ${assistant.name}`}
      className="flex max-h-full w-full max-w-lg flex-col gap-2 overflow-auto rounded-md border border-border bg-background p-4 text-sm text-foreground"
      onClose={onClose}
    >
      <h2 className="text-sm font-semibold">Edit assistant</h2>
      <div className="text-xs text-muted-foreground">
        {assistant.name} · {assistant.modelId} · {assistant.daemonInstanceId}
      </div>
      <input
        data-testid="models-edit-assistant-label"
        placeholder="Label"
        className={fieldClassName}
        value={label}
        onChange={(e) => setLabel(e.target.value)}
      />
      <textarea
        data-testid="models-edit-assistant-system-prompt"
        placeholder="System prompt"
        rows={4}
        className={fieldClassName}
        value={systemPrompt}
        onChange={(e) => setSystemPrompt(e.target.value)}
      />
      <AssistantToolPicker
        idPrefix="models-edit-assistant"
        legend="Tools — what this assistant may call itself"
        catalog={toolCatalog}
        selected={toolSets.tools}
        onToggle={toolSets.toggleTool}
      />
      <AssistantToolPicker
        idPrefix="models-edit-assistant-replaced"
        legend="Replaces — taken away from the main agent"
        catalog={toolCatalog}
        selected={toolSets.replaces}
        onToggle={toolSets.toggleReplaced}
      />
      {error ? (
        <div data-testid="models-edit-assistant-error" className="text-xs text-destructive">
          {error}
        </div>
      ) : null}
      <div className="flex items-center gap-2">
        <button
          type="button"
          data-testid="models-edit-assistant-submit"
          className="rounded-md border border-input px-3 py-1 text-sm font-medium text-foreground hover:bg-accent disabled:opacity-50"
          disabled={submitting || toolCatalog.status !== "ready"}
          onClick={() => void submit()}
        >
          {submitting ? "Saving…" : "Save"}
        </button>
        <button
          type="button"
          data-testid="models-edit-assistant-cancel"
          className="rounded-md border border-input px-3 py-1 text-sm font-medium text-foreground hover:bg-accent"
          onClick={onClose}
        >
          Cancel
        </button>
      </div>
    </ModelsDialogShell>
  );
}
