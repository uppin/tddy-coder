import type { OrchestratorDefaults, ProjectEntry } from "tddy-client-web/gen/project_pb";

/**
 * What a new orchestrator for a project starts with on one host (`#orchestrators` 11/11): the
 * composer's model, effort and provider account. Each `""` means "the agent's own default" — the
 * composer then shows what the agent serves.
 */
export interface OrchestratorDefaultsChoice {
  model: string;
  effort: string;
  providerAccountId: string;
}

/** Nothing saved: every control at the agent's own default. */
export const NO_ORCHESTRATOR_DEFAULTS: OrchestratorDefaultsChoice = {
  model: "",
  effort: "",
  providerAccountId: "",
};

/** The fields of a `ListProjects` row the defaults are read from. */
export type ProjectDefaultsRow = Pick<ProjectEntry, "projectId" | "daemonInstanceId" | "orchestratorDefaults">;

/**
 * The defaults `projectId` has saved on `daemonInstanceId`, read from a `ListProjects` answer: the
 * row for that project **on that host** — defaults are per host, as main is — or
 * {@link NO_ORCHESTRATOR_DEFAULTS} when that host lists no such row or saved none.
 */
export function orchestratorDefaultsFor(
  projects: readonly ProjectDefaultsRow[],
  projectId: string,
  daemonInstanceId: string,
): OrchestratorDefaultsChoice {
  // TODO(project-settings-pane): implement
  void projects;
  void projectId;
  void daemonInstanceId;
  return NO_ORCHESTRATOR_DEFAULTS;
}

/**
 * The `SetProjectOrchestratorDefaultsRequest.defaults` the settings pane sends for `choice`:
 * `undefined` — clear — when every control is at the agent's own default, so a project whose
 * defaults were all cleared lists none.
 */
export function orchestratorDefaultsMessage(
  choice: OrchestratorDefaultsChoice,
): Omit<OrchestratorDefaults, "$typeName" | "$unknown"> | undefined {
  // TODO(project-settings-pane): implement
  void choice;
  return undefined;
}
