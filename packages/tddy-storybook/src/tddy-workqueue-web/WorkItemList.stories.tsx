import { useState } from "react";
import type { Meta, StoryObj } from "@storybook/react";
import { WorkItemList } from "tddy-workqueue-web";
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
