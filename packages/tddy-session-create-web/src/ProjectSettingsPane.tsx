import type React from "react";
import type { ProjectEntry } from "tddy-client-web/gen/project_pb";
import type { SessionEntry } from "tddy-client-web/gen/session_pb";
import type { StartSessionRequestInit } from "./hooks/useSessionAttachments";

/** The registry row the settings pane shows — one project, on the pane's host. */
export type ProjectSettingsProject = Pick<
  ProjectEntry,
  "projectId" | "name" | "mainBranchRef" | "homeSessionId" | "orchestratorDefaults"
>;

export interface ProjectSettingsPaneProps {
  projectId: string;
  /** The registry row for `projectId` on {@link daemonInstanceId}; `null` when that host does not list it. */
  project: ProjectSettingsProject | null;
  /** Main's listed session, for its model and status; `null` when main is not set up or not listed. */
  mainSession?: Pick<SessionEntry, "sessionId" | "model" | "status" | "isActive"> | null;
  /**
   * Open with main's section already replacing it — the old `/sessions/project-home/:id/replace`
   * route. Otherwise a set-up main shows its summary and a **Replace main** action.
   */
  replacing?: boolean;
  /** The host the pane acts on: the route's `?host=`, or the drawer's selected host. */
  daemonInstanceId: string;
  sessionToken: string;
  /** Sets main up, or replaces it, from the start spec (`EnsureProjectHomeSession`). Rejects with the daemon's reason. */
  onStartMain: (spec: StartSessionRequestInit) => Promise<void>;
  /** The defaults were saved on {@link daemonInstanceId}: the answer's row, so the listing can refresh. */
  onDefaultsSaved?: (project: ProjectEntry) => void;
  onClose: () => void;
}

/**
 * A project's **session settings** (`#orchestrators` 11/11), opened by the gear on its drawer
 * header at `#/sessions/project/<projectId>/settings[?host=]` (and by the old
 * `#/sessions/project-home/<projectId>[/replace]` routes):
 *
 * - **Main** — main's setup form (`ProjectHomeSetupPane`) when main is not set up; otherwise main's
 *   model and status and **Replace main** (moved here off the pinned entry), which opens the same
 *   form in replace mode.
 * - **New orchestrators** — the defaults the "+" composer starts with: model, effort and provider
 *   account, the New-session form's own fields, each with an "agent default" choice. **Save** sends
 *   `SetProjectOrchestratorDefaults` for the pane's host; clearing every control clears them.
 *
 * The clients are the selected daemon's, as `ProjectHomeSetupPane`'s are; the request names the
 * pane's host, which routes it.
 *
 * Test ids: `sessions-project-settings-<projectId>` (the pane), `…-main-<projectId>` (main's
 * section), `…-replace-main-<projectId>`, `…-defaults-<projectId>` (the defaults section),
 * `…-defaults-save-<projectId>`, `…-defaults-error-<projectId>`, `…-defaults-saved-<projectId>`.
 *
 * PRD: docs/ft/web/1-WIP/PRD-2026-11-10-project-settings-pane.md
 */
export function ProjectSettingsPane(props: ProjectSettingsPaneProps): React.ReactElement | null {
  // TODO(project-settings-pane): implement
  void props;
  return null;
}
