/**
 * The session inspector's PR view with its commit flow (`#workqueue` 11/14): [`SessionPullRequestTab`]
 * (untouched) beside **Inbox** and **Outbox** tabs, for a session linked to a work item of a queue.
 * A session no queue links shows the PR tab alone.
 *
 * Test ids: `session-pr-flow-tab-<pr|inbox|outbox>` (the strip, `aria-selected`), `session-pr-flow-error`,
 * plus the tabs' own ([`PullRequestInboxTab`], [`PullRequestOutboxTab`]).
 */
import React, { useEffect, useState } from "react";
import type { Client } from "@connectrpc/connect";
import type { WorkQueueService } from "tddy-client-web/gen/work_queue_pb";
import { InlineError } from "tddy-ui-web";
import { rpcFailureText } from "tddy-client-web";
import { SessionPullRequestTab, type SessionPullRequestTabProps } from "./SessionPullRequestTab";
import {
  PullRequestInboxTab,
  PullRequestOutboxTab,
  type InboxCommitView,
  type OutboxView,
} from "./PullRequestInboxTab";
import { useSessionWorkItem, type SessionWorkItem } from "./useSessionWorkItem";

export interface SessionPullRequestFlowTabsProps extends SessionPullRequestTabProps {
  workQueueClient?: Client<typeof WorkQueueService>;
}

type FlowTab = "pr" | "inbox" | "outbox";

const TABS: { id: FlowTab; label: string }[] = [
  { id: "pr", label: "Pull request" },
  { id: "inbox", label: "Inbox" },
  { id: "outbox", label: "Outbox" },
];

function FlowPanel({
  tab,
  item,
  workQueueClient,
  sessionToken,
}: {
  tab: "inbox" | "outbox";
  item: SessionWorkItem;
  workQueueClient: Client<typeof WorkQueueService>;
  sessionToken: string;
}) {
  const [inbox, setInbox] = useState<InboxCommitView[] | null>(null);
  const [outbox, setOutbox] = useState<OutboxView[] | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let current = true;
    setError(null);
    const request = { sessionToken, queue: item.queue, itemId: item.itemId };
    const read =
      tab === "inbox"
        ? workQueueClient.getWorkItemInbox(request).then((res) => {
            if (!current) return;
            setInbox(
              res.commits.map((commit) => ({
                sha: commit.sha,
                subject: commit.subject,
                source: `${commit.sourceKind}:${commit.source}`,
                conflictFiles: commit.conflictFiles,
              })),
            );
          })
        : workQueueClient.getWorkItemOutbox(request).then((res) => {
            if (!current) return;
            setOutbox(
              res.successors.map((entry) => ({
                successor: entry.successor,
                commits: entry.commits.map((commit) => ({ sha: commit.sha, subject: commit.subject })),
              })),
            );
          });
    read.catch((err) => current && setError(rpcFailureText(err)));
    return () => {
      current = false;
    };
  }, [tab, item.itemId, item.queue.queueId, item.queue.projectId, item.queue.daemonInstanceId, workQueueClient, sessionToken]);

  return (
    <div className="px-3 py-3 text-xs text-muted-foreground">
      <InlineError testId="session-pr-flow-error" message={error} className="text-xs" />
      {tab === "inbox"
        ? inbox !== null && <PullRequestInboxTab inbox={inbox} />
        : outbox !== null && <PullRequestOutboxTab outbox={outbox} />}
    </div>
  );
}

export function SessionPullRequestFlowTabs({ workQueueClient, ...pr }: SessionPullRequestFlowTabsProps) {
  const item = useSessionWorkItem(workQueueClient, pr.sessionToken, pr.projectId, pr.sessionId);
  const [tab, setTab] = useState<FlowTab>("pr");

  if (!item || !workQueueClient) return <SessionPullRequestTab {...pr} />;
  return (
    <div className="flex flex-col">
      <div role="tablist" className="flex gap-3 px-3 pt-2 text-xs">
        {TABS.map(({ id, label }) => (
          <button
            key={id}
            type="button"
            role="tab"
            data-testid={`session-pr-flow-tab-${id}`}
            aria-selected={tab === id}
            className={tab === id ? "font-medium text-foreground" : "text-muted-foreground"}
            onClick={() => setTab(id)}
          >
            {label}
          </button>
        ))}
      </div>
      {tab === "pr" ? (
        <SessionPullRequestTab {...pr} />
      ) : (
        <FlowPanel tab={tab} item={item} workQueueClient={workQueueClient} sessionToken={pr.sessionToken} />
      )}
    </div>
  );
}
