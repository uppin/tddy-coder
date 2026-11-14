import React, { useState } from "react";
import type { Client } from "@connectrpc/connect";
import { AccountsService } from "tddy-client-web/gen/accounts_pb";
import { CatalogService } from "tddy-client-web/gen/catalog_pb";
import { ProjectService, type ProjectEntry } from "tddy-client-web/gen/project_pb";
import type { SessionEntry } from "tddy-client-web/gen/session_pb";
import { rpcFailureText, useDaemonClient } from "tddy-client-web";
import { Button } from "tddy-ui-web";
import type { StartSessionRequestInit } from "./hooks/useSessionAttachments";
import { ProjectHomeSetupPane } from "./ProjectHomeSetupPane";
import { CreateSessionEffortField } from "./CreateSessionEffortField";
import { BUILT_IN_PROVIDER, CreateSessionProviderField } from "./CreateSessionProviderField";
import { inputClass, labelClass } from "./createSessionFormStyles";
import { useAgentModels } from "./useAgentModels";
import { useProviderRoutes } from "./useProviderRoutes";
import { useSelectableAgents } from "./useSelectableAgents";
import { orchestratorSessionTypeFor } from "./orchestratorStartRequest";
import {
  NO_ORCHESTRATOR_DEFAULTS,
  orchestratorDefaultsMessage,
  type OrchestratorDefaultsChoice,
} from "./orchestratorDefaults";

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
  /**
   * Opened from the old setup route (`/sessions/project-home/:id`): show main's setup form even
   * though a main is recorded — that route has always opened the form.
   */
  setupForm?: boolean;
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
  const { projectId, project, daemonInstanceId, sessionToken, onStartMain, onClose } = props;
  const [replacing, setReplacing] = useState(props.replacing ?? false);
  const hasMain = Boolean(project?.homeSessionId);
  const showForm = !hasMain || replacing || (props.setupForm ?? false);

  return (
    <div className="flex flex-1 min-h-0 items-start justify-center overflow-auto p-6">
      <section
        data-testid={`sessions-project-settings-${projectId}`}
        className="w-full max-w-xl rounded-md border border-border p-4"
      >
        <h2 className="mb-3 text-base font-medium">{project ? `${project.name} settings` : "Project settings"}</h2>
        <div data-testid={`sessions-project-settings-main-${projectId}`}>
          {project !== null && !showForm ? (
            <div className="mb-4">
              <p className="mb-2 text-sm text-muted-foreground">
                <span className="font-medium text-foreground">main</span>
                {props.mainSession?.model ? ` — ${props.mainSession.model}` : ""}
                {props.mainSession?.status ? ` (${props.mainSession.status})` : ""}
              </p>
              <Button
                type="button"
                variant="secondary"
                data-testid={`sessions-project-settings-replace-main-${projectId}`}
                onClick={() => setReplacing(true)}
              >
                Replace main
              </Button>
            </div>
          ) : (
            <ProjectHomeSetupPane
              projectId={projectId}
              project={project}
              replacing={replacing && hasMain}
              daemonInstanceId={daemonInstanceId}
              sessionToken={sessionToken}
              onStart={onStartMain}
              onClose={onClose}
            />
          )}
        </div>
        {project !== null && (
          <DefaultsSection
            projectId={projectId}
            project={project}
            daemonInstanceId={daemonInstanceId}
            sessionToken={sessionToken}
            onDefaultsSaved={props.onDefaultsSaved}
          />
        )}
      </section>
    </div>
  );
}

function DefaultsSection(props: {
  projectId: string;
  project: ProjectSettingsProject;
  daemonInstanceId: string;
  sessionToken: string;
  onDefaultsSaved?: (project: ProjectEntry) => void;
}): React.ReactElement {
  const catalogClient = useDaemonClient(CatalogService);
  const accountsClient = useDaemonClient(AccountsService);
  const projectClient = useDaemonClient(ProjectService);
  if (!catalogClient || !projectClient) {
    return <div data-testid={`sessions-project-settings-defaults-${props.projectId}`} className="mt-4 border-t border-border pt-4" />;
  }
  return <DefaultsSectionBody {...props} catalogClient={catalogClient} accountsClient={accountsClient ?? undefined} projectClient={projectClient} />;
}

function DefaultsSectionBody({
  projectId,
  project,
  daemonInstanceId,
  sessionToken,
  onDefaultsSaved,
  catalogClient,
  accountsClient,
  projectClient,
}: {
  projectId: string;
  project: ProjectSettingsProject;
  daemonInstanceId: string;
  sessionToken: string;
  onDefaultsSaved?: (project: ProjectEntry) => void;
  catalogClient: Client<typeof CatalogService>;
  accountsClient?: Client<typeof AccountsService> | undefined;
  projectClient: Client<typeof ProjectService>;
}): React.ReactElement {
  const initial: OrchestratorDefaultsChoice = project.orchestratorDefaults
    ? {
        model: project.orchestratorDefaults.model ?? "",
        effort: project.orchestratorDefaults.effort ?? "",
        providerAccountId: project.orchestratorDefaults.providerAccountId ?? "",
      }
    : NO_ORCHESTRATOR_DEFAULTS;
  const [choice, setChoice] = useState<OrchestratorDefaultsChoice>(initial);
  const [error, setError] = useState<string | null>(null);
  const [saved, setSaved] = useState(false);
  const [saving, setSaving] = useState(false);

  const { agents } = useSelectableAgents(catalogClient, daemonInstanceId);
  const selectedAgent = agents.find(
    (a) => a.daemonInstanceId === daemonInstanceId && a.capabilities.includes("project_home_main"),
  );
  const sessionType = catalogClient ? orchestratorSessionTypeFor(agents, daemonInstanceId) : null;
  const agentModels = useAgentModels(catalogClient, sessionToken, sessionType ?? "", daemonInstanceId);
  const providerRoutes = useProviderRoutes(accountsClient ?? undefined, sessionToken, sessionType);

  const save = async () => {
    if (!projectClient || saving) return;
    setSaving(true);
    setError(null);
    setSaved(false);
    try {
      const resp = await projectClient.setProjectOrchestratorDefaults({
        sessionToken,
        projectId,
        daemonInstanceId,
        defaults: orchestratorDefaultsMessage(choice),
      });
      setSaved(true);
      if (resp.project) onDefaultsSaved?.(resp.project as ProjectEntry);
    } catch (err) {
      setError(rpcFailureText(err));
    } finally {
      setSaving(false);
    }
  };

  return (
    <div data-testid={`sessions-project-settings-defaults-${projectId}`} className="mt-4 border-t border-border pt-4">
      <h3 className="mb-2 text-sm font-medium">New orchestrators</h3>
      <div className="grid grid-cols-2 gap-3">
        <div>
          <label className={labelClass} htmlFor="project-settings-model">Model</label>
          {agentModels.loading ? (
            <p className="text-sm text-muted-foreground">Loading models…</p>
          ) : (
            <select
              id="project-settings-model"
              data-testid="create-session-model-select"
              className={inputClass}
              value={choice.model}
              onChange={(e) => setChoice((c) => ({ ...c, model: e.target.value }))}
            >
              <option value="">Default</option>
              {agentModels.models.map((m) => (
                <option key={m.id} value={m.id}>{m.label}</option>
              ))}
            </select>
          )}
        </div>
        <CreateSessionEffortField
          levels={selectedAgent?.effortLevels ?? []}
          effort={choice.effort}
          setEffort={(effort) => setChoice((c) => ({ ...c, effort }))}
        />
        {accountsClient && (
          <CreateSessionProviderField
            harnessRoutable={providerRoutes.harnessRoutable}
            choices={providerRoutes.choices}
            value={choice.providerAccountId === "" ? BUILT_IN_PROVIDER : choice.providerAccountId}
            onChange={(v) => setChoice((c) => ({ ...c, providerAccountId: v === BUILT_IN_PROVIDER ? "" : v }))}
            error={providerRoutes.error}
          />
        )}
      </div>
      {error !== null && (
        <p data-testid={`sessions-project-settings-defaults-error-${projectId}`} className="mt-2 text-sm text-destructive">{error}</p>
      )}
      {saved && (
        <p data-testid={`sessions-project-settings-defaults-saved-${projectId}`} className="mt-2 text-sm text-muted-foreground">Saved</p>
      )}
      <div className="mt-3 flex justify-end">
        <Button
          type="button"
          data-testid={`sessions-project-settings-defaults-save-${projectId}`}
          disabled={saving || !projectClient}
          onClick={() => void save()}
        >
          Save
        </Button>
      </div>
    </div>
  );
}
