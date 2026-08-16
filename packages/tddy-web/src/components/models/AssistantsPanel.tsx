import { safeTestIdPart } from "../../lib/testId";
import {
  assistantRowKey,
  registryEmptyStateText,
  type AssistantRow,
  type DaemonFailure,
  type RegistryReadStatus,
} from "../../utils/mergeRegistryEntries";

/**
 * The assistants defined across the fleet: each is a model plus a system prompt plus a tool set,
 * selectable afterwards as `--agent <name>` on the daemon that owns it
 * (docs/ft/web/1-WIP/PRD-2026-08-16-models-and-assistants.md § AC8, AC9).
 *
 * Like the providers panel, an empty list says which of "none defined", "still reading" and
 * "the read failed" it is — the three are the same blank panel otherwise.
 */

const actionClassName =
  "rounded-md border border-input px-2 py-1 text-xs font-medium text-foreground hover:bg-accent";

/**
 * The `data-testid` stem of an assistant row. An assistant's `--agent` name is unique *per daemon*,
 * so two hosts may each define a `reviewer`; without the owning daemon in the id their rows would
 * collide in the DOM and a spec would act on whichever one rendered first.
 */
export function assistantRowTestId(assistant: AssistantRow): string {
  return `models-assistant-row-${assistant.daemonInstanceId}-${safeTestIdPart(assistant.name)}`;
}

export interface AssistantsPanelProps {
  assistants: AssistantRow[];
  /** Errors from a write against an assistant row, by {@link assistantRowKey}. */
  assistantErrors: ReadonlyMap<string, string>;
  /** Daemons whose registry could not be read. */
  failures: DaemonFailure[];
  /** Why the panel is empty, when it is. */
  status: RegistryReadStatus;
  /**
   * Open a conversation with this assistant — its model, its system prompt and its tools. One with
   * tools is asked where they run before the chat opens; the host decides, not this panel.
   */
  onOpenChat: (assistant: AssistantRow) => void;
  onEditAssistant: (assistant: AssistantRow) => void;
  onDeleteAssistant: (assistant: AssistantRow) => void;
}

export function AssistantsPanel({
  assistants,
  assistantErrors,
  failures,
  status,
  onOpenChat,
  onEditAssistant,
  onDeleteAssistant,
}: AssistantsPanelProps) {
  return (
    <section data-testid="models-assistants-panel" className="mt-6">
      <h2 className="mb-2 text-sm font-semibold text-foreground">Assistants</h2>
      <div className="flex flex-col gap-2">
        {failures.map((failure) => (
          <div
            key={`failure-${failure.instanceId}`}
            data-testid={`models-assistants-daemon-error-${safeTestIdPart(failure.instanceId)}`}
            className="rounded-md border border-border p-3 text-sm text-destructive"
          >
            {failure.instanceId}: {failure.error}
          </div>
        ))}
        {assistants.map((assistant) => {
          const testId = assistantRowTestId(assistant);
          const error = assistantErrors.get(assistantRowKey(assistant)) ?? "";
          return (
            <div
              key={assistantRowKey(assistant)}
              data-testid={testId}
              className="rounded-md border border-border p-3 text-sm text-foreground"
            >
              <div className="flex flex-wrap items-center gap-2">
                <span className="font-medium">{assistant.label}</span>
                <span className="text-xs text-muted-foreground">{assistant.name}</span>
                <span className="text-xs text-muted-foreground">{assistant.modelId}</span>
                <span className="text-xs text-muted-foreground">{assistant.daemonInstanceId}</span>
                <button
                  type="button"
                  data-testid={`${testId}-chat`}
                  className={actionClassName}
                  onClick={() => onOpenChat(assistant)}
                >
                  Chat
                </button>
                <button
                  type="button"
                  data-testid={`${testId}-edit`}
                  className={actionClassName}
                  onClick={() => onEditAssistant(assistant)}
                >
                  Edit
                </button>
                <button
                  type="button"
                  data-testid={`${testId}-delete`}
                  className={actionClassName}
                  onClick={() => onDeleteAssistant(assistant)}
                >
                  Delete
                </button>
              </div>
              <div
                data-testid={`${testId}-tools`}
                data-assistant-tools={assistant.tools.join(",")}
                className="mt-1 text-xs text-muted-foreground"
              >
                {assistant.tools.join(" · ")}
              </div>
              {error ? (
                <div data-testid={`${testId}-error`} className="mt-1 text-xs text-destructive">
                  {error}
                </div>
              ) : null}
            </div>
          );
        })}
        {assistants.length === 0 && failures.length === 0 ? (
          <div
            data-testid="models-assistants-empty"
            data-registry-status={status}
            className="rounded-md border border-border p-3 text-sm text-muted-foreground"
          >
            {registryEmptyStateText(status, {
              loading: "Reading the fleet's assistants…",
              ready: "No assistants defined",
            })}
          </div>
        ) : null}
      </div>
    </section>
  );
}
