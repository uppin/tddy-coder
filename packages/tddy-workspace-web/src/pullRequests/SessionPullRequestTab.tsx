/**
 * The session inspector's PR tab (`#workqueue` 4/14): the PR whose head is the session's branch — its
 * state, checks, threaded comments and commits — streamed from the daemon's GitHub inbox through
 * `WorktreeService.StreamSessionPullRequest`. Reused for a work item's PR view by later nodes.
 *
 * Test ids: `session-pr-tab`, `session-pr-state`, `session-pr-check-<name>`, `session-pr-thread-<root id>`,
 * `session-pr-comment-<id>`, `session-pr-commit-<sha>`, `session-pr-none`.
 */
import React, { useEffect, useState } from "react";
import type { Client } from "@connectrpc/connect";
import type { WorktreeService } from "tddy-client-web/gen/worktree_pb";
import { InlineError } from "tddy-ui-web";
import { CheckRow } from "./CheckRow";
import { CommentThreads } from "./CommentThread";
import { CommitList } from "./CommitList";
import { usePullRequestView } from "./usePullRequestView";

export interface SessionPullRequestTabProps {
  client: Client<typeof WorktreeService> | null;
  sessionToken: string;
  projectId: string;
  sessionId: string;
}

const CLOCK_TICK_MS = 1000;

/** The wall clock, ticking, so a running check's elapsed time moves between stream frames. */
function useNowMs(): number {
  const [nowMs, setNowMs] = useState(() => Date.now());
  useEffect(() => {
    const timer = setInterval(() => setNowMs(Date.now()), CLOCK_TICK_MS);
    return () => clearInterval(timer);
  }, []);
  return nowMs;
}

function Section({ title, children }: { title: string; children: React.ReactNode }) {
  return (
    <section className="flex flex-col gap-1">
      <h3 className="font-medium text-foreground">{title}</h3>
      {children}
    </section>
  );
}

export function SessionPullRequestTab({ client, sessionToken, projectId, sessionId }: SessionPullRequestTabProps) {
  const { view, error } = usePullRequestView(client, sessionToken, projectId, sessionId);
  const nowMs = useNowMs();

  return (
    <div data-testid="session-pr-tab" className="px-3 py-3 flex flex-col gap-3 text-xs text-muted-foreground">
      <InlineError testId="session-pr-error" message={error} className="text-xs" />
      {view === null ? (
        error === null ? <p>Loading pull request…</p> : null
      ) : view.unavailableReason !== "" ? (
        <p data-testid="session-pr-unavailable">{view.unavailableReason}</p>
      ) : !view.exists ? (
        <p data-testid="session-pr-none">This session&apos;s branch has no pull request.</p>
      ) : (
        <>
          <div className="flex flex-col gap-1">
            <a
              href={view.url}
              target="_blank"
              rel="noreferrer"
              className="text-foreground underline-offset-2 hover:underline"
            >
              #{view.number.toString()} {view.title}
            </a>
            <div>
              <span data-testid="session-pr-state">{view.state}</span> · {view.headBranch} → {view.baseBranch}
            </div>
          </div>
          <Section title="Checks">
            {view.checks.length === 0 ? (
              <p>No checks.</p>
            ) : (
              <ul className="flex flex-col gap-1">
                {view.checks.map((check, index) => (
                  // Two runs may share a name (a matrix job), so the name alone is not a key.
                  <CheckRow key={`${index}-${check.name}`} check={check} nowMs={nowMs} />
                ))}
              </ul>
            )}
          </Section>
          <Section title="Comments">
            {view.comments.length === 0 ? <p>No comments.</p> : <CommentThreads comments={view.comments} />}
          </Section>
          <Section title="Commits">
            {view.commits.length === 0 ? <p>No commits.</p> : <CommitList commits={view.commits} />}
          </Section>
        </>
      )}
    </div>
  );
}
