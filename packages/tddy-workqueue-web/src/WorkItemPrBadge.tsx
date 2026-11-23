import React from "react";
import { WorkItemChecksSummary } from "./WorkItemChecksSummary";
import { useWorkItemLinks } from "./workItemLinks";
import type { WorkItem } from "./workQueue";

/** The PR a task shows: the live GitHub PR found for its branch, else the PR assigned to it. */
export interface PrBadgeSource {
  number: number | bigint;
  url: string;
  /** "open" (ready for review) | "draft" | "merged" | "closed"; empty when unknown. */
  state: string;
}

const STATE_CLASS: Record<string, string> = {
  open: "bg-green-100 text-green-800 dark:bg-green-900 dark:text-green-100",
  draft: "bg-muted text-muted-foreground",
  merged: "bg-purple-100 text-purple-800 dark:bg-purple-900 dark:text-purple-100",
  closed: "bg-red-100 text-red-800 dark:bg-red-900 dark:text-red-100",
};
const STATE_LABEL: Record<string, string> = { open: "ready for review", draft: "draft" };

/**
 * The PR of a task, inline in its box's header just before the Start/session control: the PR number
 * as a link — green when the PR is ready for review, grey when it is a draft — with its checks
 * (`WorkItemChecksSummary`: failed and succeeded counts) as a small round-cornered pill wedged into the
 * box's top edge, above the number, and an arc running along the number's bubble border while any check
 * is still running. A PR'ed task is the same box as any other, so this adds no height; nothing renders for a
 * task without a PR.
 *
 * Test ids: `work-item-pr-link-${itemId}` (text `#<number>`, `data-state`), `work-item-checks-*`.
 */
export function WorkItemPrBadge({ node, pr }: { node: WorkItem; pr?: PrBadgeSource }) {
  const checks = useWorkItemLinks()?.checksByItemId[node.itemId];
  const found: PrBadgeSource | undefined = pr ?? (node.pr ? { number: node.pr.number, url: node.pr.url, state: "" } : undefined);
  if (!found) return null;
  const state = found.state;
  const label = STATE_LABEL[state] ?? (state || "state unknown");
  return (
    <span className="relative flex flex-shrink-0 items-center">
      {checks && (checks.failed > 0 || checks.succeeded > 0) && (
        <span
          data-testid={`work-item-checks-pill-${node.itemId}`}
          className="absolute left-1/2 -translate-x-1/2 rounded-full border border-border bg-background px-1.5 py-px shadow-sm"
          style={{ bottom: "calc(100% + 7px)" }}
        >
          <WorkItemChecksSummary itemId={node.itemId} running={checks.running} failed={checks.failed} succeeded={checks.succeeded} />
        </span>
      )}
      <span className="relative flex items-center">
        {checks && checks.running > 0 && (
          <svg
            data-testid={`work-item-checks-running-${node.itemId}`}
            aria-label={`${checks.running} check${checks.running === 1 ? "" : "s"} running`}
            className="pointer-events-none absolute inset-0 h-full w-full text-amber-500"
          >
            <title>{`${checks.running} check${checks.running === 1 ? "" : "s"} running`}</title>
            {/* An arc running along the bubble's own border. */}
            <rect x="1" y="1" style={{ width: "calc(100% - 2px)", height: "calc(100% - 2px)" }} rx="11" ry="11" pathLength={100} fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeDasharray="22 78">
              <animate attributeName="stroke-dashoffset" from="0" to="-100" dur="1.2s" repeatCount="indefinite" />
            </rect>
          </svg>
        )}
        <a
          data-testid={`work-item-pr-link-${node.itemId}`}
          data-state={state}
          href={found.url}
          target="_blank"
          rel="noreferrer"
          title={`Pull request #${found.number} — ${label}`}
          className={`relative rounded-full px-2 py-0.5 font-mono text-xs hover:underline ${STATE_CLASS[state] ?? "bg-muted text-muted-foreground"}`}
        >
          #{found.number.toString()}
        </a>
      </span>
      {state && (
        <span data-testid={`work-item-pr-state-${node.itemId}`} className="sr-only">
          {state}
        </span>
      )}
    </span>
  );
}
