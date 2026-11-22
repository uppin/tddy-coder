import React, { useState } from "react";
import { Button } from "tddy-ui-web";
import { useWorkItemLinks } from "./workItemLinks";
import type { WorkItem } from "./workQueue";

/**
 * A row's annotation controls (`#workqueue` 7/14): edit the item's labels (comma-separated, saved as
 * the whole set) and remove a badge by key. Rendered only where the view supplies
 * [`WorkItemLinks`](./workItemLinks); a refusal shows in the link controls' error line.
 *
 * Test ids: `work-item-edit-labels-${id}` (opens the input), `…-input-${id}`, `…-save-${id}`,
 * `work-item-clear-badge-${id}-${key}`.
 */
export function WorkItemAnnotationControls({ node }: { node: WorkItem }) {
  const links = useWorkItemLinks();
  const [editing, setEditing] = useState(false);
  const [draft, setDraft] = useState("");
  if (!links) return null;
  const id = node.itemId;
  const save = () => {
    links.onSetLabels(
      id,
      draft
        .split(",")
        .map((label) => label.trim())
        .filter((label) => label !== ""),
    );
    setEditing(false);
  };

  return (
    <div className="mt-1 flex flex-wrap items-center gap-2 text-xs">
      {editing ? (
        <>
          <input
            data-testid={`work-item-edit-labels-input-${id}`}
            value={draft}
            onChange={(event) => setDraft(event.target.value)}
            placeholder="label, label, …"
            className="w-48 rounded border px-1 text-xs"
          />
          <Button data-testid={`work-item-edit-labels-save-${id}`} size="sm" onClick={save}>
            Save labels
          </Button>
        </>
      ) : (
        <Button
          data-testid={`work-item-edit-labels-${id}`}
          size="sm"
          variant="outline"
          onClick={() => {
            setDraft(node.labels.join(", "));
            setEditing(true);
          }}
        >
          Edit labels
        </Button>
      )}
      {node.badges.map((badge) => (
        <Button
          key={badge.key}
          data-testid={`work-item-clear-badge-${id}-${badge.key}`}
          size="sm"
          variant="outline"
          onClick={() => links.onClearBadge(id, badge.key)}
        >
          Clear {badge.key}
        </Button>
      ))}
    </div>
  );
}
