/**
 * The options a *project picker* offers, derived from aggregated `ListProjects` rows.
 *
 * Aggregated `ListProjects` returns one row per (project_id, hosting daemon) — a deliberate contract
 * (pinned by `packages/tddy-daemon/tests/list_projects_multi_daemon_aggregation.rs`), because callers
 * that route work need to know which hosts carry a project. A picker that submits only a
 * `project_id` — the host being chosen by its own selector — wants the logical project instead, so
 * rendering those rows one-to-one would show a project once per host that carries it.
 *
 * The reduction to one option per project id loses the host, which makes a *name* the only thing the
 * operator reads. Two genuinely different projects may share one (two checkouts of the same repo),
 * so a label that would be ambiguous carries its project id as well.
 */

/** One aggregated `ListProjects` row, reduced to the fields a project picker reads. */
export interface ProjectRow {
  projectId: string;
  name: string;
}

/** One selectable project: the id the form submits, and the caption it is chosen by. */
export interface ProjectOption {
  projectId: string;
  label: string;
}

/**
 * One option per logical project, in order of each project id's first appearance.
 *
 * Rows are grouped by id *before* names are compared, so the same project described differently by
 * two hosts' registries is one option — labelled by the first row's name — rather than a name
 * collision. Only when distinct projects resolve to the same caption is every one of them qualified
 * as `"<name> (<projectId>)"`; an unambiguous name is left plain. A row with no name falls back to
 * its id, which is always unique and needs no qualifying.
 */
export function projectSelectOptions(rows: readonly ProjectRow[]): ProjectOption[] {
  /** project id → its caption, keyed by insertion order of the first row carrying that id. */
  const labelsByProjectId = new Map<string, string>();
  for (const row of rows) {
    if (labelsByProjectId.has(row.projectId)) continue;
    labelsByProjectId.set(row.projectId, row.name || row.projectId);
  }

  const projectsPerLabel = new Map<string, number>();
  for (const label of labelsByProjectId.values()) {
    projectsPerLabel.set(label, (projectsPerLabel.get(label) ?? 0) + 1);
  }

  return [...labelsByProjectId].map(([projectId, label]) => ({
    projectId,
    label: (projectsPerLabel.get(label) ?? 0) > 1 ? `${label} (${projectId})` : label,
  }));
}
