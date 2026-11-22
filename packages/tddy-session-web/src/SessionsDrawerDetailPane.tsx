import React, { useState } from "react";
import type { ProjectEntry } from "tddy-client-web/gen/project_pb";
import type { SessionEntry } from "tddy-client-web/gen/session_pb";
import { OrchestratorComposer, ProjectSettingsPane } from "tddy-session-create-web";
import { SessionsDrawerUnknownSession } from "./SessionsDrawerScreenParts";
import { parseSessionsProjectNew, parseSessionsProjectSettings } from "tddy-client-web";
import type { CatalogService } from "tddy-client-web/gen/catalog_pb";
import { registerSessionTypeOptions, sessionTypeOptions, useSelectableAgents } from "tddy-session-create-web";

/**
 * The sessions drawer's detail pane (`#orchestrators` 12/12, B10): the route cases beside the
 * session list — the orchestrator composer, the project settings pane,
 * the unknown-session notice, and the session views themselves (passed in as `sessionPane`).
 * Split out of `SessionsDrawerScreen`, which keeps the drawer composition.
 */
export function SessionsDrawerDetailPane(props: {
  location: { path: string };
  setup: { projectId: string; replacing?: boolean } | null;
  sessionToken: string;
  sortedSessions: SessionEntry[];
  projects: ProjectEntry[];
  selectedInstanceId: string;
  onSessionCreated: (sessionId: string) => void;
  onStartMainFor: (projectId: string, host: string, spec: Parameters<React.ComponentProps<typeof ProjectSettingsPane>["onStartMain"]>[0]) => Promise<void>;
  /** Read the registry again: the settings pane saved a change to the project (name, defaults) or deleted it. */
  onProjectChanged: () => void;
  onCloseSettings: () => void;
  unknownSession: boolean;
  onUnknownHome: () => void;
  sessionPane: React.ReactNode;
}): React.ReactElement {
  const {
    location,
    setup,
    sessionToken,
    sortedSessions,
    projects,
    selectedInstanceId,
    onSessionCreated,
    onStartMainFor,
    onProjectChanged,
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
  const settingsForm = settingsRoute === null && setup !== null;
  const onStartMain: React.ComponentProps<typeof ProjectSettingsPane>["onStartMain"] = (spec) =>
    settingsProjectId ? onStartMainFor(settingsProjectId, settingsHost, spec) : Promise.resolve();

  return (
    <>
          {composerProjectId !== null ? (
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
              onDefaultsSaved={onProjectChanged}
              onProjectChanged={onProjectChanged}
              onDeleted={() => {
                // A deleted project leaves the drawer; its settings have nothing left to show.
                onProjectChanged();
                onCloseSettings();
              }}
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
