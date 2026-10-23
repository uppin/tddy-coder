import { useEffect, useRef, useState } from "react";
import { ConnectError } from "@connectrpc/connect";
import type { AgentInfo } from "../../gen/catalog_pb";
import type { ProjectEntry } from "../../gen/project_pb";

/**
 * The per-project **Home agent** picker (`#project-home` 1/7): `Not set` plus one option per agent
 * the catalog lists. Changing it stores the choice through `SetProjectHomeAgent`; a refusal is shown
 * on the card, and the select keeps showing the stored value. When `focused`, the select takes focus
 * and is scrolled into view as soon as it is rendered — how the sessions drawer's "main" entry points
 * an operator at a project with no home agent.
 *
 * Spec: docs/ft/web/projects-screen-multi-host.md (§ Home agent).
 */
export function ProjectHomeAgent(props: {
  project: Pick<ProjectEntry, "projectId" | "homeAgent">;
  agents: Pick<AgentInfo, "id" | "label">[];
  /** Focus the select and scroll it into view. */
  focused?: boolean;
  /** Resolves once stored; rejects with the daemon's reason when refused. */
  onChange: (agentId: string) => Promise<void>;
}) {
  const { project, agents, focused = false, onChange } = props;
  const [stored, setStored] = useState(project.homeAgent);
  const [pending, setPending] = useState(false);
  const [refusal, setRefusal] = useState<string | null>(null);

  const selectRef = useRef<HTMLSelectElement>(null);

  useEffect(() => {
    setStored(project.homeAgent);
  }, [project.homeAgent]);

  // Nothing to choose from and nothing stored: there is no picker to offer.
  const hasPicker = agents.length > 0 || stored !== "";

  // Once per arrival: re-runs only when the picker appears or the focus target changes, so a list
  // poll does not pull focus back from wherever the operator went next.
  useEffect(() => {
    if (!focused || !hasPicker) return;
    selectRef.current?.focus();
    selectRef.current?.scrollIntoView({ block: "center" });
  }, [focused, hasPicker]);

  if (!hasPicker) return null;

  // A stored id the catalog does not list is still the stored value. It is offered as its own option
  // so the select never reports the project as unset while one is stored.
  const storedIsUnlisted = stored !== "" && !agents.some((agent) => agent.id === stored);

  const choose = (agentId: string) => {
    setPending(true);
    setRefusal(null);
    onChange(agentId)
      .then(() => setStored(agentId))
      .catch((err: unknown) => setRefusal(ConnectError.from(err).rawMessage))
      .finally(() => setPending(false));
  };

  return (
    <div className="mb-3 flex flex-col gap-1 text-sm">
      <div className="flex items-center gap-2">
        <span className="text-muted-foreground">Home agent</span>
        <select
          ref={selectRef}
          data-testid={`project-home-agent-select-${project.projectId}`}
          value={stored}
          disabled={pending}
          onChange={(e) => choose(e.target.value)}
          className="rounded border border-border px-2 py-1"
        >
          <option value="">Not set</option>
          {storedIsUnlisted ? (
            <option value={stored}>{`${stored} (not listed by this daemon)`}</option>
          ) : null}
          {agents.map((agent) => (
            <option key={agent.id} value={agent.id}>
              {agent.label}
            </option>
          ))}
        </select>
      </div>
      {refusal !== null ? (
        <span
          data-testid={`project-home-agent-error-${project.projectId}`}
          className="text-destructive"
        >
          {refusal}
        </span>
      ) : null}
    </div>
  );
}
