import React, { useState } from "react";
import type { Client } from "@connectrpc/connect";
import type { PrStackService } from "tddy-client-web/gen/pr_stack_pb";
import type { ProjectEntry } from "tddy-client-web/gen/project_pb";
import type { SessionEntry } from "tddy-client-web/gen/session_pb";
import { ProjectStackView, type ProjectStackAddress } from "tddy-prstack-web";
import { OrchestratorComposer, ProjectSettingsPane } from "tddy-session-create-web";
import { SessionsDrawerUnknownSession } from "./SessionsDrawerScreenParts";
import { parseSessionsProjectNew, parseSessionsProjectSettings, parseSessionsProjectStack } from "tddy-client-web";
import type { CatalogService } from "tddy-client-web/gen/catalog_pb";
import { registerSessionTypeOptions, sessionTypeOptions, useSelectableAgents } from "tddy-session-create-web";

type StackRoute = { projectId: string; stackId: string };
/**
 * The sessions drawer's detail pane (`#orchestrators` 12/12, B10): the route cases beside the
 * session list — the project stack view, the orchestrator composer, the project settings pane,
 * the unknown-session notice, and the session views themselves (passed in as `sessionPane`).
 * Split out of `SessionsDrawerScreen`, which keeps the drawer composition.
 */
export function SessionsDrawerDetailPane(props: {
  location: { path: string };
  setup: { projectId: string; replacing?: boolean } | null;
  stackAddress: ProjectStackAddress | null;
  setStackAddress: (address: ProjectStackAddress | null) => void;
  prStackClient?: Client<typeof PrStackService> | null;
  sessionToken: string;
  sortedSessions: SessionEntry[];
  sessionMetadataBySessionId: React.ComponentProps<typeof ProjectStackView>["sessionMetadataBySessionId"];
  projects: ProjectEntry[];
  onSelectSession: (sessionId: string) => void;
  onChildSessionStarted: (entry: { sessionId: string; recipe: string; orchestratorSessionId: string; projectId: string }) => void;
  viewClients: Pick<
    React.ComponentProps<typeof ProjectStackView>,
    "client" | "projectClient" | "catalogClient" | "sessionFilesClient" | "worktreeClient" | "taskClient"
  >;
  selectedInstanceId: string;
  onSessionCreated: (sessionId: string) => void;
  onStartMainFor: (projectId: string, host: string, spec: Parameters<React.ComponentProps<typeof ProjectSettingsPane>["onStartMain"]>[0]) => Promise<void>;
  onDefaultsSaved: () => void;
  onCloseSettings: () => void;
  unknownSession: boolean;
  onUnknownHome: () => void;
  sessionPane: React.ReactNode;
}): React.ReactElement {
  const {
    location,
    setup,
    stackAddress,
    setStackAddress,
    prStackClient,
    sessionToken,
    sortedSessions,
    sessionMetadataBySessionId,
    projects,
    onSelectSession,
    onChildSessionStarted,
    viewClients,
    selectedInstanceId,
    onSessionCreated,
    onStartMainFor,
    onDefaultsSaved,
    onCloseSettings,
    unknownSession,
    onUnknownHome,
    sessionPane,
  } = props;

  const settingsRoute = parseSessionsProjectSettings(location as never);
  const settingsProjectId = settingsRoute?.projectId ?? setup?.projectId ?? null;
  const settingsHost = settingsRoute?.host ?? selectedInstanceId ?? "";
  const settingsReplacing = settingsRoute ? false : (setup?.replacing ?? false);
  const settingsProject =
    settingsProjectId !== null
      ? (projects.find(
          (p) => p.projectId === settingsProjectId && p.daemonInstanceId === settingsHost,
        ) ?? null)
      : null;
  const composerProjectId = parseSessionsProjectNew(location.path);
  const stackRoute = parseSessionsProjectStack(location.path);
  const settingsForm = settingsRoute === null && setup !== null;
  const onStartMain: React.ComponentProps<typeof ProjectSettingsPane>["onStartMain"] = (spec) =>
    settingsProjectId ? onStartMainFor(settingsProjectId, settingsHost, spec) : Promise.resolve();

  const stackRouteProjectId = stackRoute?.projectId;
  const stackRouteStackId = stackRoute?.stackId;
  React.useEffect(() => {
    if (!stackRouteProjectId || !stackRouteStackId) {
      setStackAddress(null);
      return;
    }
    if (
      stackAddress?.projectId === stackRouteProjectId &&
      stackAddress?.stackId === stackRouteStackId &&
      stackAddress?.daemonInstanceId
    ) {
      return;
    }
    const fallback: ProjectStackAddress = {
      projectId: stackRouteProjectId,
      stackId: stackRouteStackId,
      daemonInstanceId: "",
    };
    const stacksClient = prStackClient;
    if (!stacksClient) {
      setStackAddress(fallback);
      return;
    }
    let cancelled = false;
    stacksClient
      .listProjectStacks({ sessionToken, projectId: stackRouteProjectId, thisHostOnly: false })
      .then((res) => {
        if (cancelled) return;
        const summary = res.stacks.find((s) => s.address?.stackId === stackRouteStackId);
        setStackAddress(
          summary?.address
            ? {
                projectId: summary.address.projectId,
                stackId: summary.address.stackId,
                daemonInstanceId: summary.address.daemonInstanceId,
              }
            : fallback,
        );
      })
      .catch(() => {
        if (!cancelled) setStackAddress(fallback);
      });
    return () => {
      cancelled = true;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [stackRouteProjectId, stackRouteStackId, prStackClient, sessionToken]);


  return (
    <>
          {stackRoute !== null ? (
            stackAddress !== null ? (
              <ProjectStackView
                address={stackAddress}
                prStackClient={prStackClient ?? undefined}
                sessionToken={sessionToken}
                sessions={sortedSessions}
                sessionMetadataBySessionId={sessionMetadataBySessionId}
                defaultBranch={
                  projects.find((project) => project.projectId === stackRoute.projectId)
                    ?.mainBranchRef ?? ""
                }
                onOpenSession={onSelectSession}
                onChildSessionStarted={(entry) =>
                  onChildSessionStarted({ ...entry, orchestratorSessionId: "" })
                }
                client={viewClients.client}
                projectClient={viewClients.projectClient}
                catalogClient={viewClients.catalogClient}
                sessionFilesClient={viewClients.sessionFilesClient}
                worktreeClient={viewClients.worktreeClient}
                taskClient={viewClients.taskClient}
              />
            ) : null
          ) : composerProjectId !== null ? (
            <OrchestratorComposer
              projectId={composerProjectId}
              daemonInstanceId={selectedInstanceId ?? ""}
              sessionToken={sessionToken}
              onStarted={onSessionCreated}
            />
          ) : settingsProjectId !== null ? (
            <ProjectSettingsPane
              projectId={settingsProjectId}
              project={settingsProject}
              mainSession={
                settingsProject?.homeSessionId
                  ? (sortedSessions.find((s) => s.sessionId === settingsProject.homeSessionId) ?? null)
                  : null
              }
              replacing={settingsReplacing}
              setupForm={settingsForm}
              daemonInstanceId={settingsHost}
              sessionToken={sessionToken}
              onStartMain={onStartMain}
              onDefaultsSaved={onDefaultsSaved}
              onClose={onCloseSettings}
            />
          ) : unknownSession ? (
            <SessionsDrawerUnknownSession onHome={onUnknownHome} />
          ) : (
            sessionPane
          )}
    </>
  );
}

/**
 * Registers the session types the selected daemon serves, from its `ListAgents` answer, for
 * readers that hold no catalog of their own (a terminal tab's label, the "+" menu's kinds).
 * Without a listing of its own this screen left those readers with no session type until some
 * Create pane happened to mount. Renders nothing.
 */
export function SessionTypeOptionsRegistrar({
  catalogClient,
  instanceId,
}: {
  catalogClient: Client<typeof CatalogService>;
  instanceId: string;
}) {
  const { sessionTypes } = useSelectableAgents(catalogClient, instanceId);
  registerSessionTypeOptions(sessionTypeOptions(sessionTypes, instanceId, instanceId));
  return null;
}
