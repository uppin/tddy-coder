import { useEffect, useState } from "react";
import type { Client } from "@connectrpc/connect";
import {
  CodeIndexState,
  type CodeIndexPhase,
  type CodeIndexProgress,
  type CodeNavigationService,
} from "../../gen/code_navigation_pb";

/**
 * Where a worktree's code index stands, as a `CodeIndexProgress` stream reports it — the header's
 * `WatchCodeIndex(session)` or the Code pane's `CodeSession`.
 *
 * - `waiting` — nothing heard yet.
 * - `idle` — nothing holds the index (no server for the worktree, or the watch ended unheard).
 * - `none` — the worktree has no code index at all (not a Rust worktree); why.
 * - `progress` — loading: the latest progress message, naming the phase reported most recently,
 *   its percentage, and every phase running now.
 * - `ready` — the index is loaded and queryable.
 * - `failed` — the index, or the stream following it, failed; the reason.
 * - `reconnecting` — the Code pane's `CodeSession` dropped and is being reopened; why it dropped.
 *   The header's watch never reports it.
 */
export type CodeIndexStatus =
  | { kind: "waiting" }
  | { kind: "idle" }
  | { kind: "none"; reason: string }
  | { kind: "progress"; progress: CodeIndexProgress }
  | { kind: "ready" }
  | { kind: "failed"; reason: string }
  | { kind: "reconnecting"; reason: string };

const failed = (reason: string): CodeIndexStatus => ({
  kind: "failed",
  reason: reason !== "" ? reason : "the code index failed",
});

/**
 * Classify one progress message. `state` decides when the host sets it; a host that predates it
 * (`UNSPECIFIED`) is read from `error` and `ready`.
 */
export function codeIndexStatusOf(progress: CodeIndexProgress): CodeIndexStatus {
  switch (progress.state) {
    case CodeIndexState.FAILED:
      return failed(progress.error);
    case CodeIndexState.READY:
      return { kind: "ready" };
    case CodeIndexState.IDLE:
      return { kind: "idle" };
    case CodeIndexState.NONE:
      return { kind: "none", reason: progress.error };
    case CodeIndexState.LOADING:
      return { kind: "progress", progress };
    case CodeIndexState.UNSPECIFIED:
      if (progress.error !== "") return failed(progress.error);
      if (progress.ready) return { kind: "ready" };
      return { kind: "progress", progress };
  }
}

/** The status a failed stream leaves behind. */
export function codeIndexStatusOfError(err: unknown): CodeIndexStatus {
  return failed(err instanceof Error ? err.message : String(err));
}

/** `<phase> <n>%` — the phase reported most recently and its percentage. */
export function currentPhaseHeadline(progress: CodeIndexProgress): string {
  return `${progress.phase} ${progress.percentage}%`;
}

/** `<title> <n>%`, or the bare title for a phase that never reported a percentage. */
export function phaseHeadline(phase: CodeIndexPhase): string {
  return phase.percentage === undefined ? phase.title : `${phase.title} ${phase.percentage}%`;
}

/** Follow `code_navigation.WatchCodeIndex(session)` on `client` until it ends or the deps change. */
export function useCodeIndexProgress(
  client: Client<typeof CodeNavigationService>,
  sessionToken: string,
  sessionId: string,
): CodeIndexStatus {
  const [status, setStatus] = useState<CodeIndexStatus>({ kind: "waiting" });

  useEffect(() => {
    // Consumed with a `cancelled` flag rather than an `AbortSignal`: the LiveKit transport accepts a
    // signal for server-streaming calls and never reads it.
    let cancelled = false;
    let heard = false;
    setStatus({ kind: "waiting" });
    (async () => {
      try {
        for await (const progress of client.watchCodeIndex({ sessionToken, sessionId })) {
          if (cancelled) return;
          heard = true;
          setStatus(codeIndexStatusOf(progress));
        }
        // Ended without a message: nothing on the host follows this session's index (no
        // `index_daemon:`, or a worktree that is not a Cargo workspace).
        if (!cancelled && !heard) setStatus({ kind: "idle" });
      } catch (err) {
        if (!cancelled) setStatus(codeIndexStatusOfError(err));
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [client, sessionToken, sessionId]);

  return status;
}
