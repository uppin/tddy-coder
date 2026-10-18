import { useEffect, useState } from "react";
import type { Client } from "@connectrpc/connect";
import {
  CodeIndexState,
  type CodeIndexPhase,
  type CodeIndexProgress,
  type CodeNavigationService,
} from "../../gen/code_navigation_pb";
import { StreamReopener, streamEndOf, type StreamEnd } from "./streamReopening";

/**
 * Where a worktree's code index stands, as a `CodeIndexProgress` stream reports it — the header's
 * `WatchCodeIndex(session)` or the Code pane's `CodeSession`.
 *
 * - `waiting` — nothing heard yet.
 * - `idle` — nothing holds the index (no server for the worktree), or the header's watch ended and
 *   the next has not reported yet.
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

/** `<phase> <n>%` — the phase reported most recently and its percentage. */
export function currentPhaseHeadline(progress: CodeIndexProgress): string {
  return `${progress.phase} ${progress.percentage}%`;
}

/** `<title> <n>%`, or the bare title for a phase that never reported a percentage. */
export function phaseHeadline(phase: CodeIndexPhase): string {
  return phase.percentage === undefined ? phase.title : `${phase.title} ${phase.percentage}%`;
}

/**
 * Follows `code_navigation.WatchCodeIndex(session)` on `client` from `start` to `stop`, re-watching
 * whenever a watch ends — on `StreamReopener`'s schedule — unless the host refused it.
 *
 * The host never starts an index for a watch: with no index daemon running, the watch ends at once
 * and empty, and a watch also ends when the index daemon stops. Re-watching is what lets the header
 * show a load the Code pane or the agent starts later. A watch that ended reports `idle` — whatever
 * it said last (*Indexing … 40%*) is no longer known — until the next one reports.
 *
 * The LiveKit transport accepts a signal for server-streaming calls and never reads it, so `stopped`
 * drops whatever arrives after `stop`; the signal ends the call sooner on transports that read it.
 */
class CodeIndexWatcher {
  private stopped = false;
  private abort: AbortController | null = null;
  private readonly reopener = new StreamReopener();

  constructor(
    private readonly client: Client<typeof CodeNavigationService>,
    private readonly session: { sessionToken: string; sessionId: string },
    private readonly onStatus: (status: CodeIndexStatus) => void,
  ) {}

  start(): void {
    void this.follow();
  }

  stop(): void {
    this.stopped = true;
    this.reopener.stop();
    this.abort?.abort();
  }

  private async follow(): Promise<void> {
    const ended = await this.watch();
    if (ended === null || this.stopped) return;
    if (ended !== "ended" && ended.refused) {
      this.onStatus(failed(ended.reason));
      return;
    }
    // A watch that failed says why until the next one reports; one that ended knows nothing more.
    this.onStatus(ended === "ended" ? { kind: "idle" } : failed(ended.reason));
    this.reopener.reopenLater(() => void this.follow());
  }

  /** Read one watch to its end: `"ended"`, how it failed, or `null` once stopped. */
  private async watch(): Promise<"ended" | StreamEnd | null> {
    this.abort = new AbortController();
    try {
      const progress = this.client.watchCodeIndex(this.session, { signal: this.abort.signal });
      this.reopener.opened();
      for await (const message of progress) {
        if (this.stopped) return null;
        this.reopener.heard();
        this.onStatus(codeIndexStatusOf(message));
      }
      return "ended";
    } catch (err) {
      if (this.stopped) return null;
      return streamEndOf(err);
    }
  }
}

/**
 * Follow the session's code index (`code_navigation.WatchCodeIndex`) for as long as the caller is
 * mounted, re-watching after each watch ends (see `CodeIndexWatcher`). New deps start afresh.
 */
export function useCodeIndexProgress(
  client: Client<typeof CodeNavigationService>,
  sessionToken: string,
  sessionId: string,
): CodeIndexStatus {
  const [status, setStatus] = useState<CodeIndexStatus>({ kind: "waiting" });

  useEffect(() => {
    setStatus({ kind: "waiting" });
    const watcher = new CodeIndexWatcher(client, { sessionToken, sessionId }, setStatus);
    watcher.start();
    return () => watcher.stop();
  }, [client, sessionToken, sessionId]);

  return status;
}
