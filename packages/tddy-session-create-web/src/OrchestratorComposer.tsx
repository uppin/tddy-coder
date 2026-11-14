import React, { useEffect, useState } from "react";
import type { Client } from "@connectrpc/connect";
import { AccountsService } from "tddy-client-web/gen/accounts_pb";
import { CatalogService } from "tddy-client-web/gen/catalog_pb";
import { ProjectService, type ProjectEntry } from "tddy-client-web/gen/project_pb";
import { SessionFilesService } from "tddy-client-web/gen/session_files_pb";
import { SessionService } from "tddy-client-web/gen/session_pb";
import { WorktreeService } from "tddy-client-web/gen/worktree_pb";
import { hostNotConnectedText, rpcFailureText, useDaemonClient, useDaemons, useSelectedDaemon } from "tddy-client-web";
import { Button } from "tddy-ui-web";
import { AGENT_CLI_LAUNCH } from "./cliHarnessOptions";
import { inputClass } from "./createSessionFormStyles";
import { CreateSessionAttachmentsSection } from "./CreateSessionAttachmentsSection";
import { CreateSessionHostField } from "./CreateSessionHostField";
import { CreateSessionModelField } from "./CreateSessionModelField";
import { BUILT_IN_PROVIDER, CreateSessionProviderField } from "./CreateSessionProviderField";
import { useSessionAttachments } from "./hooks/useSessionAttachments";
import { buildOrchestratorComposerRequest, orchestratorSessionTypeFor } from "./orchestratorStartRequest";
import { useCreateSessionEffort } from "./useCreateSessionEffort";
import { useCreateSessionModel } from "./useCreateSessionModel";
import { useOrchestratorComposerStart } from "./useOrchestratorComposerStart";
import { useSelectableAgents } from "./useSelectableAgents";

export interface OrchestratorComposerProps {
  /** The project whose header's "+" opened the composer: the orchestrator's project. */
  projectId: string;
  /** The host the composer opens on — the drawer's selected host; its host picker can change it. */
  daemonInstanceId: string;
  sessionToken: string;
  /** The started orchestrator's id: the screen selects and attaches it as a created session. */
  onStarted: (sessionId: string) => void;
}

/**
 * The **orchestrator composer** (`#orchestrators` 8/11), opened by the "+" on a project header at
 * `/sessions/project/:projectId/new`: a heading and a prompt box centred vertically in the detail
 * pane, and under the box only these controls — **model**, **effort**, **provider account**,
 * **host** and **attachments**, the New-session form's own pieces. Send (Enter, or the send button)
 * starts an orchestrator for the project (`session_role: orchestrator`, placed by the daemon on the
 * project's home worktree) with the text as its first prompt; a refusal is shown under the box with
 * the daemon's reason and the text is kept.
 *
 * Its own component — it neither renders nor extends `CreateSessionPane` (over budget, see
 * `docs/code-issues/oversized-file-create-session-pane.md`). The clients are the selected daemon's,
 * as `ProjectHomeSetupPane`'s are.
 *
 * Test ids: `sessions-project-composer-<projectId>` (the pane), `sessions-project-composer-input`,
 * `sessions-project-composer-send`, `sessions-project-composer-error`.
 *
 * PRD: docs/ft/web/1-WIP/PRD-2026-11-10-orchestrator-composer.md
 */
export function OrchestratorComposer(props: OrchestratorComposerProps): React.ReactElement | null {
  const client = useDaemonClient(SessionService);
  const projectClient = useDaemonClient(ProjectService);
  const catalogClient = useDaemonClient(CatalogService);
  const accountsClient = useDaemonClient(AccountsService);
  const sessionFilesClient = useDaemonClient(SessionFilesService);
  const worktreeClient = useDaemonClient(WorktreeService);
  const daemons = useDaemons();

  if (!client || !projectClient || !catalogClient || !sessionFilesClient || !worktreeClient) {
    return (
      <div
        data-testid={`sessions-project-composer-${props.projectId}`}
        className="flex flex-1 min-h-0 h-full items-center justify-center p-6"
      >
        <p className="text-sm text-muted-foreground">{hostNotConnectedText(null)}</p>
      </div>
    );
  }

  return (
    <OrchestratorComposerBody
      {...props}
      client={client}
      projectClient={projectClient}
      catalogClient={catalogClient}
      accountsClient={accountsClient ?? undefined}
      sessionFilesClient={sessionFilesClient}
      worktreeClient={worktreeClient}
      daemons={daemons}
    />
  );
}

interface ComposerBodyProps extends OrchestratorComposerProps {
  client: Client<typeof SessionService>;
  projectClient: Client<typeof ProjectService>;
  catalogClient: Client<typeof CatalogService>;
  accountsClient?: Client<typeof AccountsService>;
  sessionFilesClient: Client<typeof SessionFilesService>;
  worktreeClient: Client<typeof WorktreeService>;
  daemons: ReturnType<typeof useDaemons>;
}

function OrchestratorComposerBody({
  projectId,
  daemonInstanceId,
  sessionToken,
  onStarted,
  client,
  projectClient,
  catalogClient,
  accountsClient,
  sessionFilesClient,
  worktreeClient,
  daemons,
}: ComposerBodyProps): React.ReactElement {
  const { selectedInstanceId } = useSelectedDaemon();
  const connectedInstanceId = selectedInstanceId ?? "";
  const [hostId, setHostId] = useState(daemonInstanceId);
  const [prompt, setPrompt] = useState("");
  const [projects, setProjects] = useState<ProjectEntry[]>([]);
  const [stageError, setStageError] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    projectClient
      .listProjects({ sessionToken })
      .then((resp) => {
        if (!cancelled) setProjects(resp.projects as ProjectEntry[]);
      })
      .catch(() => undefined);
    return () => {
      cancelled = true;
    };
  }, [projectClient, sessionToken]);

  const { agents } = useSelectableAgents(catalogClient, connectedInstanceId);
  const sessionType = orchestratorSessionTypeFor(agents, hostId);
  const selectedAgent = agents.find(
    (a) => a.daemonInstanceId === hostId && a.capabilities.includes("project_home_main"),
  );

  const {
    model,
    setModel,
    providerChoice,
    setProviderChoice,
    providerHarness,
    providerRoutes,
    providerAccountId,
    agentModels,
  } = useCreateSessionModel({
    catalogClient,
    accountsClient,
    sessionToken,
    sessionType: sessionType ?? "",
    sessionLaunch: AGENT_CLI_LAUNCH,
    selectedAgentId: "",
    daemonInstanceId: hostId,
    selectedAgentRouteProviders: undefined,
    cliRouteProviders: selectedAgent?.routeProviders,
    initialModel: undefined,
  });

  const { effort, effortField } = useCreateSessionEffort(selectedAgent);

  const sessionAttachments = useSessionAttachments({
    client,
    sessionFilesClient,
    sessionToken,
    sessionDaemonInstanceId: hostId,
  });

  const { start, starting, error } = useOrchestratorComposerStart(client, onStarted);
  const shownError = stageError ?? error;
  const canSend =
    prompt.trim() !== "" && sessionType !== null && !starting && sessionAttachments.problem === null;

  const submit = async () => {
    if (!canSend || sessionType === null) return;
    setStageError(null);
    let staged;
    try {
      staged = await sessionAttachments.stageAttachments();
    } catch (err) {
      setStageError(rpcFailureText(err));
      return;
    }
    await start(
      buildOrchestratorComposerRequest({
        sessionToken,
        projectId,
        daemonInstanceId: hostId,
        sessionType,
        initialPrompt: prompt,
        model,
        effort,
        providerAccountId,
        providerBuiltIn: providerChoice === BUILT_IN_PROVIDER,
        attachments: staged,
      }),
    );
  };

  return (
    <div
      data-testid={`sessions-project-composer-${projectId}`}
      className="flex flex-1 min-h-0 h-full flex-col overflow-y-auto"
    >
      <div className="flex flex-1 flex-col items-center justify-center px-6 py-6">
        <div className="flex w-full max-w-xl flex-col gap-4">
          <h2 className="text-base font-medium">New orchestrator</h2>
          <textarea
            data-testid="sessions-project-composer-input"
            className={inputClass}
            rows={4}
            value={prompt}
            onChange={(e) => setPrompt(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === "Enter" && !e.shiftKey) {
                e.preventDefault();
                void submit();
              }
            }}
            placeholder="What should the orchestrator do?"
          />
          {shownError !== null && (
            <p data-testid="sessions-project-composer-error" className="text-sm text-destructive">
              {shownError}
            </p>
          )}
          <div className="grid grid-cols-2 gap-3">
            <CreateSessionModelField agentModels={agentModels} model={model} setModel={setModel} />
            {effortField}
            {accountsClient && providerHarness !== null && (
              <CreateSessionProviderField
                harnessRoutable={providerRoutes.harnessRoutable}
                choices={providerRoutes.choices}
                value={providerChoice ?? BUILT_IN_PROVIDER}
                onChange={setProviderChoice}
                error={providerRoutes.error}
              />
            )}
            <CreateSessionHostField
              daemons={daemons}
              daemonInstanceId={hostId}
              setDaemonInstanceId={setHostId}
            />
          </div>
          <CreateSessionAttachmentsSection
            sessionAttachments={sessionAttachments}
            client={client}
            sessionFilesClient={sessionFilesClient}
            worktreeClient={worktreeClient}
            sessionToken={sessionToken}
            submitting={starting}
            projects={projects}
            projectId={projectId}
          />
          <div className="flex justify-end">
            <Button
              type="button"
              data-testid="sessions-project-composer-send"
              disabled={!canSend}
              onClick={() => void submit()}
            >
              Send
            </Button>
          </div>
        </div>
      </div>
    </div>
  );
}
