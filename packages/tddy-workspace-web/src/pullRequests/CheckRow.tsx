/**
 * One check run (`#workqueue` 4/14): queued, running with its elapsed time, or completed with its
 * conclusion and duration, linking to its details. Reused by work-item rows (n6).
 *
 * Test id: `session-pr-check-<name>`.
 */
import React from "react";
import type { PullRequestCheck } from "tddy-client-web/gen/worktree_pb";

export interface CheckRowProps {
  check: PullRequestCheck;
  /** Milliseconds since the epoch, for a running check's elapsed time. */
  nowMs: number;
}

/** `45s`, `3m`, `1h 5m` — whole units, the largest two. A negative span (clock skew) reads as `0s`. */
export function formatElapsed(ms: number): string {
  const seconds = Math.max(0, Math.floor(ms / 1000));
  if (seconds < 60) return `${seconds}s`;
  const minutes = Math.floor(seconds / 60);
  if (minutes < 60) return `${minutes}m`;
  return `${Math.floor(minutes / 60)}h ${minutes % 60}m`;
}

function spanMs(from: string, to: number | string): number | null {
  const start = Date.parse(from);
  const end = typeof to === "number" ? to : Date.parse(to);
  return Number.isNaN(start) || Number.isNaN(end) ? null : end - start;
}

function statusText(check: PullRequestCheck, nowMs: number): string {
  if (check.status === "queued") return "Queued";
  if (check.status === "in_progress") {
    const elapsed = check.startedAt === "" ? null : spanMs(check.startedAt, nowMs);
    return elapsed === null ? "Running" : `Running ${formatElapsed(elapsed)}`;
  }
  const took =
    check.startedAt === "" || check.completedAt === "" ? null : spanMs(check.startedAt, check.completedAt);
  const conclusion = check.conclusion === "" ? check.status : check.conclusion;
  return took === null ? conclusion : `${conclusion} in ${formatElapsed(took)}`;
}

export function CheckRow({ check, nowMs }: CheckRowProps) {
  return (
    <li
      data-testid={`session-pr-check-${check.name}`}
      className="flex items-baseline justify-between gap-2"
    >
      {check.detailsUrl === "" ? (
        <span className="text-foreground">{check.name}</span>
      ) : (
        <a
          href={check.detailsUrl}
          target="_blank"
          rel="noreferrer"
          className="text-foreground underline-offset-2 hover:underline"
        >
          {check.name}
        </a>
      )}
      <span className="tabular-nums">{statusText(check, nowMs)}</span>
    </li>
  );
}
