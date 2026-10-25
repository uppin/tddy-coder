/**
 * The terminal feed a daemon-served session offers, over `TerminalSessionService`.
 *
 * One bidi `StreamSessionTerminalIO` call carries the output down and the keystrokes up, and every
 * open asks for `SNAPSHOT`: the daemon answers with the screen as it is now (one repaint that
 * begins with RIS, so the terminal writes it as-is) and then only what is written after it, each
 * frame stamped with the offsets it occupies. There is no replay mode to choose and no byte resume
 * to track — a re-open is a fresh VT that is shown the current screen at once, with no resize and
 * no wait for the application to repaint.
 *
 * Docs: `packages/tddy-web/docs/terminal-session.md`.
 */

import type { MessageInitShape } from "@bufbuild/protobuf";
import type { Client } from "@connectrpc/connect";
import {
  type SessionTerminalInputSchema,
  StreamReplayMode,
  TerminalSessionService,
} from "../../gen/terminal_session_pb";
import { tddyDebug } from "../../lib/debugMask";
import { TerminalFrameContinuity } from "../../lib/terminalFrameContinuity";
import { isFrameForTerminal } from "../../lib/terminalFrameIdentity";
import { createForwardHistoryFetcher } from "../../lib/terminalHistoryLoader";
import { InputRefusalTracker } from "./inputDelivery";
import { TerminalStreamOffset } from "../../lib/terminalStreamOffset";
import type { TerminalFeed, TerminalFrame, TerminalOptions, TerminalStream } from "./terminal";

const dTerm = tddyDebug("tddy:term:feed");

/**
 * How far one terminal's output has been handed out, across opens of its feed.
 *
 * Since every open asks for a snapshot this no longer chooses what an open asks for; it records the
 * stream offset the last frame reached, held per terminal because a session has several.
 */
export class TerminalResumePoint {
  private receivedUpTo: bigint | null = null;

  /** Whether an open has already landed an offset-anchored frame. */
  get synced(): boolean {
    return this.receivedUpTo !== null;
  }

  /** The stream offset the last open reached; `0n` before the first has synced. */
  get fromOffset(): bigint {
    return this.receivedUpTo ?? 0n;
  }

  record(receivedUpTo: bigint): void {
    this.receivedUpTo = receivedUpTo;
  }
}

export interface DaemonTerminalFeedDeps {
  /** The daemon that owns the session — the host's own client, not a session participant's. */
  readonly client: Client<typeof TerminalSessionService>;
  readonly sessionId: string;

  /** Where a previous open of this same terminal got to. Advanced as frames arrive. */
  readonly resume: TerminalResumePoint;

  readonly options: TerminalOptions;
}

/** One keystroke, paste or resize waiting to go up the stream. */
interface QueuedInput {
  readonly data: Uint8Array;
  /** Cumulative bytes sent on this feed including `data`, which the daemon acks back. */
  readonly inputOffset: number;
  /** The lease at the instant of the send: it moves when another screen claims the terminal. */
  readonly controlToken: string;
}

/**
 * Open `sessionId`'s terminal on a daemon that serves `TerminalSessionService`.
 *
 * The call starts immediately — a caller that had to await it could not register an `onMessage`
 * listener before the first frame, which is the ordering `GrpcSessionTerminal` already relies on.
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
  const outbound: QueuedInput[] = [];
  let wakeInput: (() => void) | null = null;
  const wake = () => wakeInput?.();
  // Set when a frame did not continue the screen: the request half sends one `requestSnapshot`
  // message on this same stream, and the guard drops every delta until the snapshot lands.
  let snapshotWanted = false;
  // A request is outstanding until a snapshot arrives, so a run of gapped frames asks once.
  let snapshotAsked = false;
  const continuity = new TerminalFrameContinuity();

  const stream: TerminalStream = {
    send(data: Uint8Array): void {
      if (closed) return;
      inputOffset += data.length;
      // Read now, never hoisted: the lease moves when another screen claims this terminal, and the
      // daemon compares what each message presents against whatever it holds at that instant.
      outbound.push({ data, inputOffset, controlToken: options.controlToken() });
      wake();
    },
    onMessage(fn: (frame: TerminalFrame) => void): void {
      listeners.push(fn);
    },
    close(): void {
      closed = true;
      wake();
    },
  };

  const offsets = new TerminalStreamOffset(0n);
  // The daemon says on the output half when it drops an input chunk, and acks once input flows again.
  const refusals = new InputRefusalTracker((failure) => options.onInputDelivery?.(failure));
  const { cols: initialCols, rows: initialRows } = options.initialGrid ?? { cols: 0, rows: 0 };

  // Settled when the daemon stops sending — `pty_done`, or a transport that gave up. Not settled by
  // this side closing the feed: that is the pane going away, not the session ending.
  let reportEnded = () => {};
  const ended = new Promise<void>((resolve) => {
    reportEnded = resolve;
  });

  /**
   * The request half: the opening message, then one message per send.
   *
   * The opening message names the terminal, the lease and the measured grid, and asks for the
   * current screen. The daemon resizes the PTY to that grid *before* it snapshots (so the screen is
   * serialized at the width this pane draws at, never re-wrapped into it) — and only for the screen
   * driving the session: without a lease that may drive it, the grid is ignored and watching
   * another screen's session does not reflow it.
   */
  async function* requests(): AsyncGenerator<MessageInitShape<typeof SessionTerminalInputSchema>> {
    yield {
      sessionToken: options.sessionToken,
      sessionId,
      terminalId,
      initialCols,
      initialRows,
      mode: StreamReplayMode.SNAPSHOT,
      controlToken: options.controlToken(),
    };
    while (!closed) {
      if (snapshotWanted) {
        snapshotWanted = false;
        // No data: asking is a read, so it needs no lease and is never refused as a write.
        yield {
          sessionToken: options.sessionToken,
          sessionId,
          terminalId,
          requestSnapshot: true,
          controlToken: options.controlToken(),
        };
        continue;
      }
      const next = outbound.shift();
      if (next === undefined) {
        await new Promise<void>((resolve) => {
          wakeInput = resolve;
        });
        wakeInput = null;
        continue;
      }
      yield {
        sessionToken: options.sessionToken,
        sessionId,
        terminalId,
        data: next.data,
        inputOffset: BigInt(next.inputOffset),
        controlToken: next.controlToken,
      };
    }
  }

  void (async () => {
    try {
      for await (const output of client.streamSessionTerminalIO(requests())) {
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
        if (refusals.observe(output)) continue;
        // An ACK frame carries neither bytes nor an anchor. It is the daemon confirming applied
        // input, and there is nowhere on a `TerminalFrame` to put it — see the changeset.
        if (output.data.length === 0 && output.endOffset === 0n) continue;
        // A frame that does not continue what is painted is dropped, never drawn over a screen it
        // was not computed against; the first one asks for a fresh snapshot, and the daemon's
        // answer re-anchors the guard.
        if (continuity.accept(output) === "gap") {
          if (!snapshotAsked) {
            snapshotAsked = true;
            snapshotWanted = true;
            wake();
          }
          dTerm("gap — dropping frame and asking for a snapshot start=%s end=%s", output.startOffset, output.endOffset);
          continue;
        }
        if (output.snapshot) snapshotAsked = false;
        resume.record(offsets.accept(output));
        const frame: TerminalFrame = {
          data: output.data,
          endOffset: output.endOffset,
          atOldest: output.atOldest,
          snapshot: output.snapshot,
          startOffset: output.startOffset,
        };
        for (const fn of listeners) fn(frame);
      }
    } catch (err) {
      dTerm(
        "streamSessionTerminalIO ended sessionId=%s terminalId=%s error=%o",
        sessionId,
        terminalId,
        err instanceof Error ? err.message : err,
      );
    } finally {
      // A feed this side closed is not a session that ended — the pane is going away.
      const closedByCaller = closed;
      closed = true;
      wake();
      if (!closedByCaller) reportEnded();
    }
  })();

  return {
    stream,
    ended,
    // This daemon holds the capture ring, so it can always replay. The fetcher is anchor-neutral —
    // whether the fill is bounded by the snapshot's `endOffset` or runs to the capture tip is the
    // loader's decision, not this one's.
    history: createForwardHistoryFetcher(client, {
      sessionToken: options.sessionToken,
      sessionId,
      terminalId,
    }),
  };
}
