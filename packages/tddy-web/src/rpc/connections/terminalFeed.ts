/**
 * The terminal feed a daemon-served session offers, over `ConnectionService`.
 *
 * This is `GrpcSessionTerminal`'s stream construction, moved off the component and onto the
 * connection. Nothing about the wire changes: the same `StreamTerminalOutput` open, the same
 * TAIL-then-`FROM_OFFSET` resume, the same per-frame identity guard, the same
 * `GetTerminalHistory` forward fill. What changes is who owns it — a component that renders bytes
 * has no business choosing a replay mode, and once the connection chooses it the *same* terminal
 * component can be fed by a wire that is not this one.
 *
 * Docs: `packages/tddy-web/docs/terminal-session.md`.
 */

import type { Client } from "@connectrpc/connect";
import { ConnectionService, StreamReplayMode } from "../../gen/connection_pb";
import { tddyDebug } from "../../lib/debugMask";
import { isFrameForTerminal } from "../../lib/terminalFrameIdentity";
import { createForwardHistoryFetcher } from "../../lib/terminalHistoryLoader";
import { TerminalStreamOffset } from "../../lib/terminalStreamOffset";
import type { TerminalFeed, TerminalFrame, TerminalOptions, TerminalStream } from "./terminal";

const dTerm = tddyDebug("tddy:term:feed");

/**
 * How much of one terminal's output a connection has already handed out, across stream opens.
 *
 * The counter `GrpcSessionTerminal` kept in `currentOffsetRef` / `hasSyncedRef`, held by the
 * connection instead of by a component's refs. It is what makes a re-open a *resume*: the second
 * open asks for `FROM_OFFSET` at the byte the first reached, so the daemon sends only the gap and
 * the terminal is not handed a replay it has already painted. Held per terminal, because a session
 * has several and each is at its own offset.
 */
export class TerminalResumePoint {
  private receivedUpTo: bigint | null = null;

  /** Whether an open has already landed its offset-anchored frame. */
  get synced(): boolean {
    return this.receivedUpTo !== null;
  }

  /** The byte the next open resumes from; `0n` before the first has synced. */
  get fromOffset(): bigint {
    return this.receivedUpTo ?? 0n;
  }

  record(receivedUpTo: bigint): void {
    this.receivedUpTo = receivedUpTo;
  }
}

export interface DaemonTerminalFeedDeps {
  /** The daemon that owns the session — the host's own client, not a session participant's. */
  readonly client: Client<typeof ConnectionService>;
  readonly sessionId: string;

  /** Where a previous open of this same terminal got to. Advanced as frames arrive. */
  readonly resume: TerminalResumePoint;

  readonly options: TerminalOptions;
}

/**
 * Open `sessionId`'s terminal on a daemon that serves `ConnectionService`.
 *
 * The output stream starts immediately — a caller that had to await it could not register an
 * `onMessage` listener before the first frame, which is the ordering `GrpcSessionTerminal` already
 * relies on.
 */
export function openDaemonTerminalFeed({
  client,
  sessionId,
  resume,
  options,
}: DaemonTerminalFeedDeps): TerminalFeed {
  const terminalId = options.terminalId ?? "";
  const listeners: Array<(frame: TerminalFrame) => void> = [];
  let closed = false;
  // Cumulative bytes written to the PTY, which the daemon acks back on the output stream. Assigned
  // here rather than by the caller so that a single feed is the only thing counting: two counters
  // over one stream would ack each other's bytes.
  let inputOffset = 0;

  const stream: TerminalStream = {
    send(data: Uint8Array): void {
      if (closed) return;
      inputOffset += data.length;
      void client
        .sendTerminalInput({
          sessionToken: options.sessionToken,
          sessionId,
          terminalId,
          data,
          // Read now, never hoisted: the lease moves when another screen claims this terminal, and
          // the daemon compares what this call presents against whatever it holds at this instant.
          controlToken: options.controlToken(),
          inputOffset: BigInt(inputOffset),
        })
        .catch((err) => {
          dTerm(
            "sendTerminalInput failed sessionId=%s terminalId=%s error=%o",
            sessionId,
            terminalId,
            err instanceof Error ? err.message : err,
          );
        });
    },
    onMessage(fn: (frame: TerminalFrame) => void): void {
      listeners.push(fn);
    },
    close(): void {
      closed = true;
    },
  };

  // Read before the first await: a `FROM_OFFSET` resume is a decision about what this open should
  // ask for, and the open's own frames start advancing the resume point the moment they land.
  const mode = resume.synced ? StreamReplayMode.FROM_OFFSET : StreamReplayMode.TAIL;
  const fromOffset = resume.fromOffset;
  const offsets = new TerminalStreamOffset(fromOffset);
  const { cols: initialCols, rows: initialRows } = options.initialGrid ?? { cols: 0, rows: 0 };

  // Settled when the daemon stops sending — `pty_done`, or a transport that gave up. Not settled by
  // this side closing the feed: that is the pane going away, not the session ending.
  let reportEnded = () => {};
  const ended = new Promise<void>((resolve) => {
    reportEnded = resolve;
  });

  void (async () => {
    try {
      // `initialCols`/`initialRows` are sent on every open and honoured by the daemon only on a
      // TAIL one — the PTY is resized *before* the replay, which is what stopped a buffer captured
      // at one width being re-wrapped into a terminal of another (the 220-column garbling).
      for await (const output of client.streamTerminalOutput({
        sessionToken: options.sessionToken,
        sessionId,
        terminalId,
        initialCols,
        initialRows,
        mode,
        fromOffset,
      })) {
        if (closed) break;
        // Every frame is stamped with the terminal it came from. One that is not this terminal's is
        // dropped here rather than painted; the stream stays open, since a mis-routed frame says
        // nothing about the frames after it.
        if (!isFrameForTerminal(output, { sessionId, terminalId })) {
          dTerm(
            "dropping foreign frame — frame=%s/%s feed=%s/%s bytes=%d",
            output.sessionId,
            output.terminalId,
            sessionId,
            terminalId,
            output.data.length,
          );
          continue;
        }
        // An ACK frame carries neither bytes nor an anchor. It is the daemon confirming applied
        // input, and there is nowhere on a `TerminalFrame` to put it — see the changeset.
        if (output.data.length === 0 && output.endOffset === 0n) continue;
        resume.record(offsets.accept(output));
        const frame: TerminalFrame = {
          data: output.data,
          endOffset: output.endOffset,
          atOldest: output.atOldest,
        };
        for (const fn of listeners) fn(frame);
      }
    } catch (err) {
      dTerm(
        "streamTerminalOutput ended sessionId=%s terminalId=%s error=%o",
        sessionId,
        terminalId,
        err instanceof Error ? err.message : err,
      );
    } finally {
      if (!closed) reportEnded();
    }
  })();

  return {
    stream,
    ended,
    // This daemon holds the capture ring, so it can always replay. The fetcher is anchor-neutral —
    // whether the fill is bounded by the replay frame's `endOffset` or runs to the capture tip is
    // the loader's decision, not this one's.
    history: createForwardHistoryFetcher(client, {
      sessionToken: options.sessionToken,
      sessionId,
      terminalId,
    }),
  };
}
