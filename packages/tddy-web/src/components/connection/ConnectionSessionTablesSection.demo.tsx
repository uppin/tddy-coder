import { create } from "@bufbuild/protobuf";
import { useMemo } from "react";
import {
  AgentInfoSchema,
  EligibleDaemonEntrySchema,
  ProjectEntrySchema,
  SessionEntrySchema,
  ToolInfoSchema,
} from "../../gen/connection_pb";
import { ConnectionSessionTablesSection } from "./ConnectionSessionTablesSection";
import { defaultProjectSessionForm } from "./projectSessionForm";

const storyNoopAsync = async () => {};

export const storyTools = [
  create(ToolInfoSchema, { path: "/usr/bin/tddy-coder", label: "tddy-coder" }),
];
export const storyAgents = [create(AgentInfoSchema, { id: "claude", label: "Claude (opus)" })];
export const storyDaemons = [
  create(EligibleDaemonEntrySchema, {
    instanceId: "daemon-local",
    label: "Local daemon",
    isLocal: true,
  }),
];

export const storyDemoProject = create(ProjectEntrySchema, {
  projectId: "proj-1",
  name: "Demo project",
  gitUrl: "https://github.com/demo/app.git",
  mainRepoPath: "/home/dev/app",
});

export const storySessionForProject = create(SessionEntrySchema, {
  sessionId: "session-abc-1",
  createdAt: "2026-03-21T12:00:00Z",
  status: "active",
  repoPath: "/home/dev/app",
  pid: 4242,
  isActive: true,
  projectId: "proj-1",
  daemonInstanceId: "daemon-local",
  workflowGoal: "Ship responsive tables",
  workflowState: "Running",
  elapsedDisplay: "3m",
  agent: "claude",
  model: "opus",
});

export const storyOrphanSession = create(SessionEntrySchema, {
  sessionId: "orphan-xyz-1",
  createdAt: "2026-03-20T09:00:00Z",
  status: "exited",
  repoPath: "/tmp/other",
  pid: 0,
  isActive: false,
  projectId: "unknown-project-id",
  daemonInstanceId: "daemon-local",
  workflowGoal: "—",
  workflowState: "Idle",
  elapsedDisplay: "—",
  agent: "—",
  model: "—",
});

/**
 * Same tree as Storybook stories: session tables in a width-constrained box. Column visibility is
 * enforced via CSS container queries on the session-tables host (`@/sessionTableColumns`).
 * Exported for Cypress component tests; Storybook imports this module too.
 */
export function SessionTablesStoryLayout(props: { outerWidthPx: number }) {
  const projectForms = useMemo(
    () => ({ "proj-1": defaultProjectSessionForm(storyTools, storyAgents, storyDaemons) }),
    [],
  );

  return (
    <div
      style={{
        width: props.outerWidthPx,
        maxWidth: "100%",
        padding: 12,
        border: "1px dashed color-mix(in oklab, var(--color-muted-foreground) 40%, transparent)",
        borderRadius: 8,
      }}
    >
      <ConnectionSessionTablesSection
        projects={[storyDemoProject]}
        sessions={[storySessionForProject, storyOrphanSession]}
        tools={storyTools}
        agents={storyAgents}
        daemons={storyDaemons}
        projectForms={projectForms}
        loading={false}
        orphanSessionDebug={false}
        onOrphanSessionDebugChange={() => {}}
        onUpdateProjectForm={() => {}}
        onStartSession={storyNoopAsync}
        onConnectSession={storyNoopAsync}
        onResumeSession={storyNoopAsync}
        onSignalSession={storyNoopAsync}
        onDeleteSession={storyNoopAsync}
        onShowWorkflowFiles={() => {}}
      />
    </div>
  );
}
