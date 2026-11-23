import { useState } from "react";
import type { Meta, StoryObj } from "@storybook/react";
import { WorkItemChainHealthContext, WorkItemLinkControls, WorkItemLinksContext, WorkItemList } from "tddy-workqueue-web";
import type { WorkItemLinks } from "tddy-workqueue-web";
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
 * PR data on work items: each task's PR and its checks — running, failed and completed — as the
 * queue's status stream reports them. "Show checks" expands each run.
 */
export const PullRequestChecks: Story = {
  render: () => (
    <WorkItemLinksContext.Provider value={PR_LINKS}>
      <div style={{ display: "flex", flexDirection: "column", gap: 12, padding: 12 }}>
        {PR_NODES.map((node) => (
          <div key={node.itemId} className="rounded-md border border-border px-3 py-2">
            <p className="text-sm font-medium">{node.title}</p>
            <WorkItemLinkControls node={node} />
          </div>
        ))}
      </div>
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
