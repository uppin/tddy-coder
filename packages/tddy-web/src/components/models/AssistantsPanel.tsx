import type { AssistantRow } from "../../utils/mergeRegistryEntries";

/**
 * The assistants defined across the fleet: each is a model plus a system prompt plus a tool set,
 * selectable afterwards as `--agent <name>` on the daemon that owns it
 * (docs/ft/web/1-WIP/PRD-2026-08-16-models-and-assistants.md § AC8, AC9).
 */
export function AssistantsPanel({ assistants }: { assistants: AssistantRow[] }) {
  return (
    <section data-testid="models-assistants-panel" className="mt-6">
      <h2 className="mb-2 text-sm font-semibold text-foreground">Assistants</h2>
      <div className="flex flex-col gap-2">
        {assistants.map((assistant) => (
          <div
            key={`${assistant.daemonInstanceId}/${assistant.name}`}
            data-testid={`models-assistant-row-${assistant.name}`}
            className="rounded-md border border-border p-3 text-sm text-foreground"
          >
            <div className="flex flex-wrap items-center gap-2">
              <span className="font-medium">{assistant.label}</span>
              <span className="text-xs text-muted-foreground">{assistant.name}</span>
              <span className="text-xs text-muted-foreground">{assistant.modelId}</span>
              <span className="text-xs text-muted-foreground">{assistant.daemonInstanceId}</span>
            </div>
            <div
              data-testid={`models-assistant-tools-${assistant.name}`}
              data-assistant-tools={assistant.tools.join(",")}
              className="mt-1 text-xs text-muted-foreground"
            >
              {assistant.tools.join(" · ")}
            </div>
          </div>
        ))}
      </div>
    </section>
  );
}
