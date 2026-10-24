import React, { useState } from "react";
import type { AgentInfo } from "../../gen/catalog_pb";
import type { ProjectEntry } from "../../gen/project_pb";
import { ProjectHomeAgent } from "../projects/ProjectHomeAgent";
import { Button } from "../ui/button";

/**
 * The detail pane a click on a project's pinned **main** entry opens when the project has no home
 * agent (`/sessions/project-home/:projectId`): the project's **Home agent** picker, focused, and a
 * **Start main session** button that is enabled once a home agent is stored. Starting selects the
 * home session, which replaces this pane; a refusal is shown here. A project id the registry does
 * not list says so instead of offering a picker for nothing.
 *
 * Feature doc: docs/ft/web/session-drawer.md § Pinned main entry.
 */
export function ProjectHomeSetupPane(props: {
  projectId: string;
  /** The registry row for `projectId`; `null` when this host does not list it. */
  project: Pick<ProjectEntry, "projectId" | "name" | "homeAgent" | "mainBranchRef"> | null;
  agents: Pick<AgentInfo, "id" | "label">[];
  /** Resolves once stored; rejects with the daemon's reason when refused. */
  onSetHomeAgent: (agentId: string) => Promise<void>;
  /** Start, resume or reach the home session. Rejects with the daemon's reason. */
  onStart: () => Promise<void>;
  onClose: () => void;
}): React.ReactElement {
  const { projectId, project, agents, onSetHomeAgent, onStart, onClose } = props;
  const [starting, setStarting] = useState(false);
  const [startError, setStartError] = useState<string | null>(null);

  const start = () => {
    setStartError(null);
    setStarting(true);
    onStart()
      .catch((e: unknown) => setStartError(e instanceof Error ? e.message : String(e)))
      .finally(() => setStarting(false));
  };

  return (
    <div className="flex flex-1 min-h-0 items-start justify-center overflow-auto p-6">
      <section
        data-testid={`sessions-project-home-setup-${projectId}`}
        aria-labelledby={`sessions-project-home-setup-title-${projectId}`}
        className="w-full max-w-md rounded-md border border-border p-4"
      >
        <h2 id={`sessions-project-home-setup-title-${projectId}`} className="mb-1 text-base font-medium">
          {project ? `Set up main for ${project.name}` : "Set up main"}
        </h2>
        {project === null ? (
          <p data-testid={`sessions-project-home-setup-unknown-${projectId}`} className="mb-3 text-sm text-muted-foreground">
            This host does not list project {projectId}.
          </p>
        ) : (
          <>
            <p className="mb-3 text-sm text-muted-foreground">
              <span className="font-medium text-foreground">main</span> is this project's long-running session
              on {project.mainBranchRef || "its main branch"}. Choose the agent it runs.
            </p>
            <ProjectHomeAgent project={project} agents={agents} focused onChange={onSetHomeAgent} />
            {agents.length === 0 && project.homeAgent === "" && (
              <p data-testid={`sessions-project-home-setup-no-agents-${projectId}`} className="mb-3 text-sm text-muted-foreground">
                This host lists no agents to choose from.
              </p>
            )}
          </>
        )}
        {startError !== null && (
          <p data-testid={`sessions-project-home-setup-error-${projectId}`} role="alert" className="mb-3 text-sm text-destructive">
            {startError}
          </p>
        )}
        <div className="flex gap-2">
          <Button
            type="button"
            data-testid={`sessions-project-home-setup-start-${projectId}`}
            disabled={project === null || project.homeAgent === "" || starting}
            onClick={start}
          >
            Start main session
          </Button>
          <Button type="button" variant="secondary" data-testid={`sessions-project-home-setup-close-${projectId}`} onClick={onClose}>
            Cancel
          </Button>
        </div>
      </section>
    </div>
  );
}
