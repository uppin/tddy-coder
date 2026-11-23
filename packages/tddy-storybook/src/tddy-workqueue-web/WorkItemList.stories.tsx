import { useState } from "react";
import type { Meta, StoryObj } from "@storybook/react";
import { WorkItemChainHealthContext, WorkItemLinksContext, WorkItemList } from "tddy-workqueue-web";
import type { WorkItemLinks } from "tddy-workqueue-web";
import { create } from "@bufbuild/protobuf";
import { SessionEntrySchema } from "tddy-client-web/gen/session_pb";
import { SESSION_INDICATOR_DOT_STYLES, SessionIndicatorDot, sessionNotificationRegistry } from "tddy-session-web";
import type { WorkItem } from "tddy-workqueue-web";

/**
 * The work queue's task list as the session sidebar shows it: one box per row top-to-bottom with a
 * small gap, a drag handle at each left edge, and a bracket on the right gutter between two tasks
 * linked by a PR-stack line edge. Tasks bound to a PR stack have a disabled handle; the free task
 * can be dragged.
 */

function item(itemId: string, title: string, overrides: Partial<WorkItem> = {}): WorkItem {
  return {
    itemId,
    title,
    description: "",
    branchSuggestion: null,
    branch: null,
    sessionId: null,
    parents: [],
    prStatus: null,
    childState: null,
    childRecipe: "tdd",
    internalStatus: null,
    displayOrder: null,
    pr: null,
    linkedSessions: [],
    labels: [],
    badges: [],
    ...overrides,
  };
}

const NODES: WorkItem[] = [
  item("n1", "Token store", { branch: "feat/token-store", displayOrder: 0 }),
  item("n2", "Auth middleware", { branch: "feat/middleware", parents: ["n1"], displayOrder: 1 }),
  item("n3", "Free task — drag me", { displayOrder: 2 }),
  item("n4", "Logout", { branch: "feat/logout", parents: ["n2"], displayOrder: 3 }),
];

const meta: Meta<typeof WorkItemList> = {
  title: "WorkQueue/WorkItemList",
  component: WorkItemList,
  decorators: [
    (Story) => (
      <div style={{ width: 440 }}>
        <Story />
      </div>
    ),
  ],
};
export default meta;

type Story = StoryObj<typeof WorkItemList>;

/** A drop moves the task `delta` rows in the persisted order, as the daemon's reorder would. */
function Draggable() {
  const [nodes, setNodes] = useState(NODES);
  const move = (itemId: string, delta: number) =>
    setNodes((prev) => {
      const ids = [...prev].sort((a, b) => a.displayOrder! - b.displayOrder!).map((n) => n.itemId);
      const from = ids.indexOf(itemId);
      ids.splice(from, 1);
      ids.splice(Math.max(0, Math.min(ids.length, from + delta)), 0, itemId);
      return prev.map((n) => ({ ...n, displayOrder: ids.indexOf(n.itemId) }));
    });
  return (
    <WorkItemList
      nodes={nodes}
      onStartSession={() => undefined}
      startingNodeId={null}
      connectors={[
        { kind: "line", from: "n1", to: "n2" },
        { kind: "line", from: "n2", to: "n4" },
      ]}
      onMove={move}
    />
  );
}

export const StackWithAFreeTask: Story = { render: () => <Draggable /> };

/** Every lineage state a row and the gutter can show, one task each. */
const LINEAGE_NODES: WorkItem[] = [
  item("l1", "Linear: on its base", { branch: "feat/a", displayOrder: 0 }),
  item("l2", "Behind its base by 3", { branch: "feat/b", parents: ["l1"], displayOrder: 1 }),
  item("l3", "Conflicting with its base", { branch: "feat/c", parents: ["l2"], displayOrder: 2 }),
  item("l4", "Leaked commits from two ancestors", { branch: "feat/d", parents: ["l3"], displayOrder: 3 }),
  item("l5", "Line unknown (base unreadable)", { branch: "feat/e", displayOrder: 4 }),
  item("l6", "Needs l1 (dependency, not a line)", { branch: "feat/f", displayOrder: 5 }),
  item("l7", "Free: no branch, no line", { displayOrder: 6 }),
];

const LINEAGE_HEALTH = new Map([
  ["l1", { leaks: [], line: { state: "linear" as const } }],
  ["l2", { leaks: [], line: { state: "behind" as const, by: 3 } }],
  ["l3", { leaks: [], line: { state: "conflicting" as const, by: 2 } }],
  [
    "l4",
    {
      leaks: [
        { sha: "a1b2c3d4e5f6", ancestor: "feat/b" },
        { sha: "0f9e8d7c6b5a", ancestor: "feat/a" },
      ],
      line: { state: "linear" as const },
    },
  ],
  ["l5", { leaks: [], line: { state: "unknown" as const } }],
]);

/**
 * Lineage states, drawn on the lines: solid brackets for PR-stack line edges (l1 → l2 → l3 → l4) each
 * carrying a linearity icon (linear, behind, conflicting) and a leak icon naming the ancestors commits
 * leaked from, a dashed bracket for a stored dependency (l1 → l6), a round corner badge on a box
 * for the commits it is behind, and a free task with no lineage at all.
 */
export const LineageStates: Story = {
  render: () => (
    <WorkItemChainHealthContext.Provider value={LINEAGE_HEALTH}>
      <WorkItemList
        nodes={LINEAGE_NODES}
        onStartSession={() => undefined}
        startingNodeId={null}
        connectors={[
          { kind: "line", from: "l1", to: "l2" },
          { kind: "line", from: "l2", to: "l3" },
          { kind: "line", from: "l3", to: "l4" },
          { kind: "dependency", from: "l1", to: "l6" },
        ]}
        onMove={() => undefined}
      />
    </WorkItemChainHealthContext.Provider>
  ),
  decorators: [
    (Story) => (
      <div style={{ width: 640 }}>
        <Story />
      </div>
    ),
  ],
};

const pr = (number: number, headBranch: string) => ({ number, url: `https://github.com/acme/app/pull/${number}`, headBranch });
const run = (name: string, status: string, conclusion: string) =>
  ({ name, status, conclusion, startedAt: "", completedAt: "", detailsUrl: "" }) as never;

const PR_NODES: WorkItem[] = [
  item("p1", "Checks running", { branch: "feat/running", pr: pr(101, "feat/running") }),
  item("p2", "Checks failed", { branch: "feat/failed", pr: pr(102, "feat/failed") }),
  item("p3", "Checks completed", { branch: "feat/done", pr: pr(103, "feat/done") }),
];

const noop = () => undefined;
const PR_LINKS: WorkItemLinks = {
  sessions: [],
  errorByItemId: {},
  onAssignPr: noop,
  onUnassignPr: noop,
  onLinkSession: noop,
  onUnlinkSession: noop,
  onSetLabels: noop,
  onClearBadge: noop,
  checksByItemId: {
    p1: { running: 2, failed: 0, succeeded: 3, runs: [run("Rust tests", "in_progress", ""), run("Web build", "completed", "success"), run("Lint", "queued", "")] },
    p2: { running: 0, failed: 2, succeeded: 4, runs: [run("Rust tests", "completed", "failure"), run("Web tests", "completed", "failure"), run("Lint", "completed", "success")] },
    p3: { running: 0, failed: 0, succeeded: 6, runs: [run("Rust tests", "completed", "success"), run("Web tests", "completed", "success")] },
  },
};

/**
 * PR data on work items: a PR'ed task is the same box with more data — its PR number just before the
 * Start control, then its checks as coloured numbers (red failed, green succeeded) and a spinner with
 * the running count while any are in flight. Expand a row to list each run.
 */
export const PullRequestChecks: Story = {
  render: () => (
    <WorkItemLinksContext.Provider value={PR_LINKS}>
      <WorkItemList
        nodes={PR_NODES}
        onStartSession={() => undefined}
        startingNodeId={null}
        connectors={[]}
        onMove={() => undefined}
      />
    </WorkItemLinksContext.Provider>
  ),
  decorators: [
    (Story) => (
      <div style={{ width: 480 }}>
        <Story />
      </div>
    ),
  ],
};

const PR_BY_LINEAGE_ID: Record<string, number> = { l1: 201, l2: 202, l3: 203, l4: 204, l5: 205, l6: 206 };
/**
 * Attached sessions, one per activity state the drawer's dot paints: l1 working (green, breathing),
 * l2 connected and idle (green), l3 needs input (yellow), l4 disconnected (grey).
 */
const ATTACHED: Record<string, { sessionId: string; isActive: boolean; needsInput?: boolean; working?: boolean }> = {
  l1: { sessionId: "sess-l1", isActive: true, working: true },
  l2: { sessionId: "sess-l2", isActive: true },
  l3: { sessionId: "sess-l3", isActive: true, needsInput: true },
  l4: { sessionId: "sess-l4", isActive: false },
};
const ATTACHED_SESSIONS = Object.fromEntries(
  Object.values(ATTACHED).map((a) => [
    a.sessionId,
    create(SessionEntrySchema, { sessionId: a.sessionId, isActive: a.isActive, pendingElicitation: Boolean(a.needsInput), status: a.isActive ? "active" : "exited" }),
  ]),
);
const renderSessionIndicator = (sessionId: string) => {
  const session = ATTACHED_SESSIONS[sessionId];
  if (!session) return null;
  if (ATTACHED[Object.keys(ATTACHED).find((k) => ATTACHED[k].sessionId === sessionId)!]?.working) {
    sessionNotificationRegistry.recordActivity(sessionId, Date.now());
  }
  return <SessionIndicatorDot session={session} sizeClassName="h-7 w-7" testId={`work-item-session-indicator-${sessionId}`} />;
};
const COMBINED_NODES = LINEAGE_NODES.map((node) => ({
  ...node,
  ...(PR_BY_LINEAGE_ID[node.itemId] ? { pr: pr(PR_BY_LINEAGE_ID[node.itemId], node.branch ?? "") } : {}),
  ...(ATTACHED[node.itemId] ? { sessionId: ATTACHED[node.itemId].sessionId, childState: ATTACHED[node.itemId].working ? "Implementing" : "Idle" } : {}),
}));
/** PR state per branch: ready for review (green) or draft (grey), plus merged and closed. */
const PR_STATE: Record<string, string> = { "feat/a": "open", "feat/b": "draft", "feat/c": "open", "feat/d": "draft", "feat/e": "merged", "feat/f": "closed" };
const COMBINED_RESOLUTIONS = Object.fromEntries(
  COMBINED_NODES.filter((n) => n.branch).map((n) => [
    n.branch!,
    {
      branch: n.branch!,
      pr: { exists: true, number: BigInt(n.pr?.number ?? 0), url: n.pr?.url ?? "", state: PR_STATE[n.branch!] ?? "open" },
      session: ATTACHED[n.itemId]
        ? { exists: true, sessionId: ATTACHED[n.itemId].sessionId, isActive: ATTACHED[n.itemId].isActive }
        : { exists: false },
    },
  ]),
) as never;
const COMBINED_LINKS: WorkItemLinks = {
  ...PR_LINKS,
  checksByItemId: {
    l1: { running: 0, failed: 0, succeeded: 5, runs: [] },
    l2: { running: 3, failed: 0, succeeded: 2, runs: [] },
    l3: { running: 0, failed: 2, succeeded: 3, runs: [] },
    l4: { running: 1, failed: 1, succeeded: 6, runs: [] },
    l6: { running: 0, failed: 0, succeeded: 4, runs: [] },
  },
};

/**
 * Lineage on the lines and PR data in the boxes, together: the full queue as the sidebar shows it. The
 * PR number is green when ready for review, grey when a draft (purple merged, red closed); its checks
 * are a pill wedged into the box's top edge; a task with an attached session shows the drawer's activity dot, scaled to the Play
 * button's size, in its place — green and breathing while the session works, green when idle, yellow
 * when it needs input, grey when disconnected.
 */
export const LineageWithPullRequests: Story = {
  render: () => (
    <WorkItemLinksContext.Provider value={COMBINED_LINKS}>
      <style>{SESSION_INDICATOR_DOT_STYLES}</style>
      <WorkItemChainHealthContext.Provider value={LINEAGE_HEALTH}>
        <WorkItemList
          nodes={COMBINED_NODES}
          branchResolutionByBranch={COMBINED_RESOLUTIONS}
          renderSessionIndicator={renderSessionIndicator}
          onStartSession={() => undefined}
          startingNodeId={null}
          connectors={[
            { kind: "line", from: "l1", to: "l2" },
            { kind: "line", from: "l2", to: "l3" },
            { kind: "line", from: "l3", to: "l4" },
            { kind: "dependency", from: "l1", to: "l6" },
          ]}
          onMove={() => undefined}
        />
      </WorkItemChainHealthContext.Provider>
    </WorkItemLinksContext.Provider>
  ),
  decorators: [
    (Story) => (
      <div style={{ width: 520 }}>
        <Story />
      </div>
    ),
  ],
};
