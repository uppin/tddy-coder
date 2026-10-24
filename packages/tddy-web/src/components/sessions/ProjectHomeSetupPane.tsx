import React from "react";
import type { ProjectEntry } from "../../gen/project_pb";
import type { StartSessionRequestInit } from "../../hooks/useSessionAttachments";
import { useHostLabel } from "../../rpc/selectedDaemon";
import { hostNotConnectedText } from "../../rpc/rpcFailure";
import { Button } from "../ui/button";
import { CreateSessionPane } from "./CreateSessionPane";
import { useSessionsDrawerClients } from "./useSessionsDrawerClients";

/**
 * The detail pane that sets up a project's **main** session (`/sessions/project-home/:projectId`),
 * or replaces it (`…/replace`): what main is, and the **New session** form in its main variant —
 * project and host fixed, only the session types that can run main selectable. **Start main
 * session** hands the form's start spec to `onStart` (`EnsureProjectHomeSession`); the started
 * session is selected, which replaces this pane, and a refusal is shown in the form. A project id
 * the registry does not list says so instead of offering a form for nothing, and so does a host no
 * wire reaches any more (the registry keeps its last good list, so the project can still be named).
 *
 * Feature doc: docs/ft/web/session-drawer.md § Pinned main entry.
 * PRD: docs/ft/web/1-WIP/PRD-2026-10-24-project-home-from-new-session-form.md
 */
export function ProjectHomeSetupPane(props: ProjectHomeSetupPaneProps): React.ReactElement {
  const { projectId, project, replacing, onClose } = props;
  const verb = replacing ? "Replace" : "Set up";

  return (
    <div className="flex flex-1 min-h-0 items-start justify-center overflow-auto p-6">
      <section
        data-testid={`sessions-project-home-setup-${projectId}`}
        aria-labelledby={`sessions-project-home-setup-title-${projectId}`}
        className="w-full max-w-xl rounded-md border border-border p-4"
      >
        <h2 id={`sessions-project-home-setup-title-${projectId}`} className="mb-1 text-base font-medium">
          {project ? `${verb} main for ${project.name}` : `${verb} main`}
        </h2>
        {project === null ? (
          <SetupNotice
            testId={`sessions-project-home-setup-unknown-${projectId}`}
            text={`This host does not list project ${projectId}.`}
            onClose={onClose}
          />
        ) : (
          <ProjectHomeSetupBody {...props} project={project} />
        )}
      </section>
    </div>
  );
}

interface ProjectHomeSetupPaneProps {
  projectId: string;
  /** The registry row for `projectId`; `null` when this host does not list it. */
  project: SetupProject | null;
  /** The project has a main, which starting stops and replaces. */
  replacing: boolean;
  /** The host main runs on. */
  daemonInstanceId: string;
  sessionToken: string;
  /** Sets main up from the start spec. Rejects with the daemon's reason. */
  onStart: (spec: StartSessionRequestInit) => Promise<void>;
  onClose: () => void;
}

type SetupProject = Pick<ProjectEntry, "projectId" | "name" | "mainBranchRef">;

/** What main is, the replacing warning, and the main variant of the form over the host's clients. */
function ProjectHomeSetupBody({
  projectId,
  project,
  replacing,
  daemonInstanceId,
  sessionToken,
  onStart,
  onClose,
}: ProjectHomeSetupPaneProps & { project: SetupProject }): React.ReactElement {
  const { client, projectClient, catalogClient, accountsClient, sessionFilesClient, worktreeClient } =
    useSessionsDrawerClients();
  const hostLabel = useHostLabel();
  const reachable = client && projectClient && catalogClient && sessionFilesClient && worktreeClient;

  return (
    <>
      <p className="mb-3 text-sm text-muted-foreground">
        <span className="font-medium text-foreground">main</span> is this project's long-running session
        on {project.mainBranchRef || "its main branch"}. Choose how it runs.
      </p>
      {replacing && (
        <p
          data-testid={`sessions-project-home-setup-replacing-${projectId}`}
          role="note"
          className="mb-3 text-sm text-amber-600 dark:text-amber-400"
        >
          Starting stops the current main. It stays in the list as an ordinary session.
        </p>
      )}
      {reachable ? (
        <CreateSessionPane
          // A fresh form per project: the variant fixes its project when it mounts.
          key={projectId}
          client={client}
          projectClient={projectClient}
          catalogClient={catalogClient}
          accountsClient={accountsClient ?? undefined}
          sessionFilesClient={sessionFilesClient}
          worktreeClient={worktreeClient}
          sessionToken={sessionToken}
          onCancel={onClose}
          // Never called: the main variant starts through `projectHome.onStart`.
          onCreated={() => undefined}
          projectHome={{ projectId, daemonInstanceId, onStart }}
        />
      ) : (
        <SetupNotice
          testId={`sessions-project-home-setup-unreachable-${projectId}`}
          text={hostNotConnectedText(hostLabel(daemonInstanceId))}
          onClose={onClose}
        />
      )}
    </>
  );
}

/** Why there is nothing to set up here, and the way back out. */
function SetupNotice({
  testId,
  text,
  onClose,
}: {
  testId: string;
  text: string;
  onClose: () => void;
}): React.ReactElement {
  return (
    <>
      <p data-testid={testId} className="mb-3 text-sm text-muted-foreground">
        {text}
      </p>
      <Button type="button" variant="secondary" onClick={onClose}>
        Cancel
      </Button>
    </>
  );
}
