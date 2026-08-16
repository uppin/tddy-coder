import type { ToolCatalog } from "./useModelRegistryFanOut";

/**
 * The tool choices for an assistant: the **daemon's** exec catalog as advertised by
 * `ListAssignableTools`, never a list the web keeps of its own.
 *
 * The catalog is rendered as the three things it can be. A daemon that has not answered yet and a
 * daemon whose catalog could not be read both offer no tools, and neither is the claim "this daemon
 * assigns no tools" — an assistant created from either would be stored toolless and would look like
 * one the operator meant to make that way.
 */

/** What the picker says when it has no catalog to show, and why it has none. */
function toolCatalogNote(catalog: ToolCatalog): string {
  switch (catalog.status) {
    case "loading":
      return "Reading the daemon's tool catalog…";
    case "unavailable":
      return `Tool catalog unavailable — ${catalog.error}`;
    default:
      return "";
  }
}

export function AssistantToolPicker({
  idPrefix,
  catalog,
  selected,
  onToggle,
}: {
  /** Stem of the `data-testid`s this picker emits, so two dialogs can render one each. */
  idPrefix: string;
  catalog: ToolCatalog;
  selected: string[];
  onToggle: (toolName: string, checked: boolean) => void;
}) {
  return (
    <fieldset
      data-testid={`${idPrefix}-tools`}
      data-tool-catalog-status={catalog.status}
      className="flex flex-col gap-1"
    >
      <legend className="text-xs text-muted-foreground">Tools</legend>
      {catalog.status === "ready" ? (
        catalog.tools.map((tool) => (
          <label key={tool.name} className="flex items-center gap-2 text-xs">
            <input
              type="checkbox"
              data-testid={`${idPrefix}-tool-${tool.name}`}
              checked={selected.includes(tool.name)}
              onChange={(e) => onToggle(tool.name, e.target.checked)}
            />
            <span>{tool.name}</span>
            {tool.isMutating ? <span className="text-muted-foreground">(mutating)</span> : null}
          </label>
        ))
      ) : (
        <span
          className={
            catalog.status === "unavailable"
              ? "text-xs text-destructive"
              : "text-xs text-muted-foreground"
          }
        >
          {toolCatalogNote(catalog)}
        </span>
      )}
    </fieldset>
  );
}
