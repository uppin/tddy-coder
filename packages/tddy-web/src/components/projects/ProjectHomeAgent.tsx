import type { AgentInfo } from "../../gen/catalog_pb";
import type { ProjectEntry } from "../../gen/project_pb";

/**
 * The per-project **Home agent** picker (`#project-home` 1/7): `Not set` plus one option per agent
 * the catalog lists. Changing it stores the choice through `SetProjectHomeAgent`; a refusal is shown
 * on the card, and the select keeps showing the stored value.
 *
 * PRD: docs/ft/web/1-WIP/PRD-2026-10-22-project-home-agent-setting.md.
 */
export function ProjectHomeAgent(props: {
  project: Pick<ProjectEntry, "projectId" | "homeAgent">;
  agents: Pick<AgentInfo, "id" | "label">[];
  /** Resolves once stored; rejects with the daemon's reason when refused. */
  onChange: (agentId: string) => Promise<void>;
}): JSX.Element | null {
  // TODO(home-agent-setting): implement
  void props;
  return null;
}
