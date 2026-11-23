import type { Meta, StoryObj } from "@storybook/react";
import { create } from "@bufbuild/protobuf";
import type { Client } from "@connectrpc/connect";
import { SessionEntrySchema, type SessionEntry } from "tddy-client-web/gen/session_pb";
import {
  WorkQueueSummarySchema,
  type WorkQueueService,
  type WorkQueueSummary,
} from "tddy-client-web/gen/work_queue_pb";
import { SessionDrawer } from "tddy-session-web";
import { TooltipProvider } from "tddy-ui-web";

/**
 * The sessions drawer with realistic data, for visual review: several projects (some with live
 * sessions, some empty), PR stacks in two projects, and a long tail of remaining sessions.
 */

const PROJECTS = [
  { projectId: "p-editor", name: "custom-editor-playground" },
  { projectId: "p-eye", name: "eye-get-it" },
  { projectId: "p-mazep", name: "mazep-map" },
  { projectId: "p-qape", name: "qape-hq" },
  { projectId: "p-surmap", name: "surmap" },
  { projectId: "p-tddy", name: "tddy-coder" },
];

let seq = 0;
function aSession(projectId: string, displayName: string, live: boolean, extra: Partial<SessionEntry> = {}) {
  seq += 1;
  return create(SessionEntrySchema, {
    sessionId: `0190${String(seq).padStart(4, "0")}-0000-7000-8000-00000000${String(seq).padStart(4, "0")}`,
    projectId,
    displayName,
    createdAt: new Date(Date.UTC(2026, 10, 1, 9, seq)).toISOString(),
    status: live ? "active" : "exited",
    isActive: live,
    pid: live ? 4000 + seq : 0,
    repoPath: `/home/dev/.worktrees/${displayName.replace(/\W+/g, "-")}`,
    ...extra,
  });
}

const YOLO = aSession("p-tddy", "yolo-pr-stack: PR stack orchestration", true, { ownsWorkQueue: true });
const SIDEBAR_STACK = aSession("p-tddy", "workqueue sidebar UX", true, { ownsWorkQueue: true });

const SESSIONS: SessionEntry[] = [
  YOLO,
  SIDEBAR_STACK,
  aSession("p-tddy", "fix: terminate kills tmux agent", true),
  ...Array.from({ length: 40 }, (_, i) => aSession("p-tddy", `old task #${i + 1}`, false)),
  aSession("p-qape", "qape planning", true),
  ...Array.from({ length: 12 }, (_, i) => aSession("p-qape", `qape chore ${i + 1}`, false)),
  aSession("p-mazep", "map tiles", false),
  aSession("p-mazep", "map legend", false),
  // A replaced main (project-home role → house icon) and an orchestrator merely named "home" (no icon)
  aSession("p-editor", "home", false, { sessionRole: "project-home" }),
  aSession("p-editor", "home", true, { sessionRole: "orchestrator" }),
];

function aStack(projectId: string, queueId: string, title: string, total: number): WorkQueueSummary {
  return create(WorkQueueSummarySchema, { address: { projectId, queueId }, title, nodeCounts: { total } });
}

const STACKS: Record<string, WorkQueueSummary[]> = {
  "p-tddy": [aStack("p-tddy", YOLO.sessionId, "", 7), aStack("p-tddy", SIDEBAR_STACK.sessionId, "Sidebar UX", 2)],
  "p-qape": [aStack("p-qape", "0190ffff-0000-7000-8000-000000000001", "Onboarding flow", 4)],
};

/** A work-queue client double answering `ListProjectWorkQueues` from {@link STACKS}. */
const workQueueClient = {
  listProjectWorkQueues: async ({ projectId }: { projectId: string }) => ({ stacks: STACKS[projectId] ?? [] }),
} as unknown as Client<typeof WorkQueueService>;

const meta: Meta<typeof SessionDrawer> = {
  title: "tddy-session-web/SessionDrawer",
  component: SessionDrawer,
  decorators: [
    (Story) => (
      <TooltipProvider>
        <div style={{ height: 900, display: "flex" }}>
          <Story />
        </div>
      </TooltipProvider>
    ),
  ],
  args: {
    sessions: SESSIONS,
    projects: PROJECTS,
    selectedSessionId: null,
    isOpen: true,
    crossHostSessionsVisible: false,
    workQueueClient,
    onSelectSession: () => {},
    onClose: () => {},
    onOpen: () => {},
    onCreateSession: () => {},
    onNewOrchestrator: () => {},
    onOpenProjectSettings: () => {},
    onOpenProjectStack: () => {},
    onEnterSelectionMode: () => {},
  },
};

export default meta;

type Story = StoryObj<typeof SessionDrawer>;

/** Many projects, live sessions in three, stacks in two, a long remaining tail. */
export const ManyProjects: Story = {};

/** The same list with no stacks anywhere — no Stacks box above Select. */
export const NoStacks: Story = {
  args: { workQueueClient: null },
};

// ---------------------------------------------------------------------------
// Home sessions in every activity state
// ---------------------------------------------------------------------------

const HOME_PROJECTS = [
  { projectId: "h-live", name: "home-connected" },
  { projectId: "h-input", name: "home-needs-input" },
  { projectId: "h-done", name: "home-finished" },
  { projectId: "h-main", name: "pinned-main" },
];

const PINNED_MAIN = aSession("h-main", "main", true, { sessionRole: "project-home", model: "opus" });

const HOME_SESSIONS: SessionEntry[] = [
  aSession("h-live", "home", true, { sessionRole: "project-home" }),
  aSession("h-input", "home", true, { sessionRole: "project-home", pendingElicitation: true }),
  aSession("h-done", "home", false, { sessionRole: "project-home" }),
  PINNED_MAIN,
  // An orchestrator merely named "home": a plain green dot, no house
  aSession("h-live", "home", true, { sessionRole: "orchestrator" }),
];

/**
 * A project-home session's house icon is its activity indicator, in place of the dot: green when
 * connected, yellow when it waits for input, grey when finished. The pinned **main** entry shows its
 * session's house the same way; an orchestrator named "home" keeps a plain dot.
 */
export const HomeSessionStates: Story = {
  args: {
    sessions: HOME_SESSIONS,
    projects: HOME_PROJECTS,
    workQueueClient: null,
    projectHomes: [{ projectId: "h-main", mainBranchRef: "origin/master", homeSessionId: PINNED_MAIN.sessionId }],
    onEnsureProjectHome: async () => {},
  },
};
