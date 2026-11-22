import React, { useState } from "react";
import { Button } from "tddy-ui-web";
import { WorkItemChecksSummary } from "./WorkItemChecksSummary";
import { useWorkItemLinks } from "./workItemLinks";
import type { WorkItem } from "./workQueue";

/**
 * A row's link controls (`#workqueue` 6/14): the PR assigned to the item (assign by number, remove),
 * the sessions linked to it (pick one to link, remove one), and — for an item with a PR — its checks.
 * Rendered only where the view supplies [`WorkItemLinks`](./workItemLinks).
 *
 * Test ids: `work-item-assign-pr-${id}` (opens the input), `…-input-${id}`, `…-submit-${id}`,
 * `work-item-unassign-pr-${id}`, `work-item-link-session-${id}` (the picker),
 * `work-item-unlink-session-${id}-${sessionId}`, `work-item-link-error-${id}`.
 */
export function WorkItemLinkControls({ node }: { node: WorkItem }) {
  const links = useWorkItemLinks();
  const [assigning, setAssigning] = useState(false);
  const [pullNumber, setPullNumber] = useState("");
  if (!links) return null;
  const id = node.itemId;
  const checks = links.checksByItemId[id];
  const error = links.errorByItemId[id];
  const parsed = Number.parseInt(pullNumber, 10);
  const linkedIds = new Set(node.linkedSessions.map((linked) => linked.sessionId));
  const offered = links.sessions.filter((session) => !linkedIds.has(session.sessionId));

  return (
    <div className="mt-1 flex flex-col gap-1">
      {node.pr ? (
        <p className="flex items-center gap-2 text-xs text-muted-foreground">
          <a
            data-testid={`work-item-assigned-pr-${id}`}
            href={node.pr.url}
            target="_blank"
            rel="noreferrer"
            className="font-mono underline"
          >
            PR #{node.pr.number}
          </a>
          <Button
            data-testid={`work-item-unassign-pr-${id}`}
            size="sm"
            variant="outline"
            onClick={() => links.onUnassignPr(id)}
          >
            Remove PR
          </Button>
        </p>
      ) : assigning ? (
        <div className="flex items-center gap-2">
          <input
            data-testid={`work-item-assign-pr-input-${id}`}
            type="number"
            min={1}
            value={pullNumber}
            onChange={(event) => setPullNumber(event.target.value)}
            placeholder="PR number"
            className="w-28 rounded border px-1 text-xs"
          />
          <Button
            data-testid={`work-item-assign-pr-submit-${id}`}
            size="sm"
            disabled={!Number.isInteger(parsed) || parsed < 1}
            onClick={() => {
              links.onAssignPr(id, parsed);
              setAssigning(false);
              setPullNumber("");
            }}
          >
            Assign
          </Button>
        </div>
      ) : (
        <Button
          data-testid={`work-item-assign-pr-${id}`}
          size="sm"
          variant="outline"
          className="self-start"
          onClick={() => setAssigning(true)}
        >
          Assign a PR
        </Button>
      )}
      {node.pr && checks && <WorkItemChecksSummary itemId={id} {...checks} />}
      {node.linkedSessions.map((linked) => (
        <p key={linked.sessionId} className="flex items-center gap-2 text-xs text-muted-foreground">
          <span className="truncate font-mono" title={linked.sessionId}>
            linked: {linked.sessionId}
          </span>
          <Button
            data-testid={`work-item-unlink-session-${id}-${linked.sessionId}`}
            size="sm"
            variant="outline"
            onClick={() => links.onUnlinkSession(id, linked.sessionId)}
          >
            Unlink
          </Button>
        </p>
      ))}
      <select
        data-testid={`work-item-link-session-${id}`}
        value=""
        className="self-start rounded border px-1 text-xs"
        onChange={(event) => {
          const session = offered.find((candidate) => candidate.sessionId === event.target.value);
          if (session) links.onLinkSession(id, session);
        }}
      >
        <option value="">Link a session…</option>
        {offered.map((session) => (
          <option key={session.sessionId} value={session.sessionId}>
            {session.label}
          </option>
        ))}
      </select>
      {error && (
        <p data-testid={`work-item-link-error-${id}`} className="text-xs text-destructive">
          {error}
        </p>
      )}
    </div>
  );
}
