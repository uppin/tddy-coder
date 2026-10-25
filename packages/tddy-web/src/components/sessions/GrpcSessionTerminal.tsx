import React, { useCallback, useEffect, useMemo, useRef, useState } from "react";
import type { MessageInitShape } from "@bufbuild/protobuf";
import type { Client } from "@connectrpc/connect";
import {
  type SessionTerminalInputSchema,
  SessionTerminalOutput,
  StreamReplayMode,
  TerminalSessionService,
} from "../../gen/terminal_session_pb";
import { GhosttyTerminalSession } from "../GhosttyTerminalSession";
import type {
  TerminalFeed,
  TerminalFrame,
  TerminalHistoryFetcher,
  TerminalStream,
} from "../../rpc/connections/terminal";
import type { ConnectedSession } from "./useTerminalControl";
import type { ToolShortcutDef } from "../../lib/toolShortcuts";
import { tddyDebug } from "../../lib/debugMask";
import { measureTerminalGridFromRect } from "../../lib/terminalGridMeasure";
import { TerminalFrameContinuity } from "../../lib/terminalFrameContinuity";
import { isFrameForTerminal } from "../../lib/terminalFrameIdentity";
import { useEnqueuedInput } from "./useEnqueuedInput";
import { EnqueuedInputOverlay } from "../connection/EnqueuedInputOverlay";
import { createForwardHistoryFetcher } from "../../lib/terminalHistoryLoader";
import { rpcFailureText } from "../../rpc/rpcFailure";
import { InputRefusalTracker } from "../../rpc/connections/inputDelivery";
import { InlineError } from "../ui/inline-error";

const dGrpc = tddyDebug("tddy:term:grpc");
const dResize = tddyDebug("tddy:term:resize");

/** `ESC ] resize;` — the prefix of the grid report `GhosttyTerminalSession` writes into the feed. */
const RESIZE_OSC_PREFIX = new TextEncoder().encode("\x1b]resize;");

function isResizeOsc(data: Uint8Array): boolean {
  return (
    data.length > RESIZE_OSC_PREFIX.length &&
    RESIZE_OSC_PREFIX.every((byte, i) => data[i] === byte)
  );
}

type TerminalClient = Client<typeof TerminalSessionService>;

/** One keystroke, paste or resize waiting to go up the stream. */
interface QueuedInput {
  readonly data: Uint8Array;
  /** Cumulative bytes sent on this terminal including `data`, which the daemon acks back. */
  readonly inputOffset: number;
  /** The lease at the instant of the send: it moves when another screen claims the terminal. */
  readonly controlToken: string;
}

interface GrpcSessionTerminalProps {
  sessionId: string;
  sessionToken: string;
  /** The daemon `TerminalSessionService` client. `null` while the transport is in a transient blip — the
   *  terminal stays mounted (its scrollback and the ghostty instance survive) and re-opens with a
   *  screen snapshot when a non-null client returns. Only a stream-end with a *valid* client (real
   *  `pty_done`) evicts the runtime. */
  client: TerminalClient | null;
  /** The claimed session (lease token in hand), or `null` while the auto-claim is in flight / was
   *  denied. `sendTerminalInput` is gated on this: input sent before the claim resolves (typically
   *  the onReady resize OSC) is queued and flushed once the claim arrives, so it can never go out
   *  with an empty/stale token. The stream itself opens immediately, presenting an empty token until
   *  the claim resolves (a watcher: the daemon ignores its grid). */
  connected: ConnectedSession | null;
  /** Target terminal within the session. Empty (the default) resolves to the reserved main
   *  ("claude"/Agent) terminal on the daemon; a bash terminal passes its own `terminal_id`. */
  terminalId?: string;
  onDisconnect?: () => void;
  mobileShortcuts?: ToolShortcutDef[];
  /** Called with this terminal's text-insert function (see `GhosttyTerminalSession.onRegisterInsertInput`),
   *  so the runtime can expose it to the inspector's Files-tab click/tap route. */
  onRegisterInsertInput?: (insertInput: (text: string) => void) => void;
  /** See `GhosttyTerminalSession.readsLocalPaths`. */
  readsLocalPaths?: boolean;
}

export function GrpcSessionTerminal({
  sessionId,
  sessionToken,
  client,
  connected,
  terminalId = "",
  onDisconnect,
  mobileShortcuts,
  onRegisterInsertInput,
  readsLocalPaths,
}: GrpcSessionTerminalProps) {
  const [stream, setStream] = useState<TerminalStream | null>(null);
  // containerRef must be on a div that is ALWAYS rendered (not gated on stream),
  // so getBoundingClientRect() returns real dimensions when the effect runs.
  const containerRef = useRef<HTMLDivElement>(null);
  // Latest claimed lease — a ref so the send() closure (created once for the stream's lifetime)
  // reads the current value without recreating the stream on each claim transition. `null` until
  // the claim resolves.
  const connectedRef = useRef<ConnectedSession | null>(connected);
  connectedRef.current = connected;
  // Latest daemon client — a ref so the stream effect (keyed on `client`) and the send() closure
  // read the current value. `null` during a transient transport blip; the terminal stays mounted
  // and re-opens with a snapshot when a non-null client returns.
  const clientRef = useRef<TerminalClient | null>(client);
  clientRef.current = client;
  const sessionTokenRef = useRef(sessionToken);
  sessionTokenRef.current = sessionToken;
  // Input sent before the claim resolves (or while the client is null on a transport blip) is
  // queued here and flushed once `connected` and a non-null client both arrive.
  const pendingInputRef = useRef<Uint8Array[]>([]);
  // The last grid this terminal reported (its resize OSC). The daemon applies a resize only from
  // the screen driving the session, so one reported while another screen drove it — or while this
  // screen was displaced — never took effect; it is re-sent each time this screen gains the lease,
  // or the PTY stays at the previous driver's size and every tap lands on the wrong cell.
  const lastResizeRef = useRef<Uint8Array | null>(null);

  // The input half of the open bidi stream: `send` queues a message the stream's request iterator
  // yields. `null` while no stream is open (a blip) — input is then held in `pendingInputRef`.
  const openInputRef = useRef<((message: QueuedInput) => void) | null>(null);

  // Enqueued-input accounting: assigns each sent chunk a cumulative byte offset, consumes the
  // daemon's acked_input_offset, and drives the un-acknowledged-input overlay on slow links.
  const { enqueue, ack, model: enqueuedModel, visible: enqueuedVisible } = useEnqueuedInput();
  const ackRef = useRef(ack);
  ackRef.current = ack;

  // Why the last keystroke, paste or resize did not reach the PTY — the daemon's refusal frame (e.g.
  // another screen took the lease) or the stream itself failing. One indicator however many sends
  // are refused, cleared when an ACK passes the refused offset or a stream opens again.
  const [inputError, setInputError] = useState<string | null>(null);

  // Disconnect fires automatically when the terminal goes away — either the remote
  // stream ends or this component unmounts (e.g. the user switches sessions). Read
  // the latest callback via a ref and guard so it fires at most once. CRITICAL: a stream-end with
  // a *null* client (transient transport blip) must NOT evict — the terminal pauses and waits for
  // a non-null client to return, then re-opens with a snapshot. Only a stream-end with a valid
  // client (real `pty_done`) evicts the runtime.
  const onDisconnectRef = useRef(onDisconnect);
  onDisconnectRef.current = onDisconnect;
  const disconnectedRef = useRef(false);
  const emitDisconnect = () => {
    if (disconnectedRef.current) return;
    if (clientRef.current === null) {
      dGrpc("stream-end with null client (transport blip) — pausing, not evicting");
      return;
    }
    disconnectedRef.current = true;
    onDisconnectRef.current?.();
  };

  const sendInputRequest = useCallback(
    (data: Uint8Array) => {
      const conn = connectedRef.current;
      const sendOnOpenStream = openInputRef.current;
      if (conn === null || clientRef.current === null || sendOnOpenStream === null) {
        // No lease yet (auto-claim in flight) OR no open stream (transient transport blip). Queue
        // the input (typically the onReady resize OSC) until both arrive and the flush effect below
        // releases it.
        pendingInputRef.current.push(data);
        return;
      }
      // Assign this chunk its cumulative byte offset (also tracked for the enqueued-input overlay)
      // and send it up the stream so the daemon can ack the applied offset back on the output.
      sendOnOpenStream({ data, inputOffset: enqueue(data), controlToken: conn.controlToken });
    },
    [enqueue],
  );
  const sendInputRequestRef = useRef(sendInputRequest);
  sendInputRequestRef.current = sendInputRequest;

  useEffect(() => {
    // No client during a transient transport blip — keep the previous stream (and the ghostty
    // terminal instance) mounted; re-open with a snapshot when a non-null client returns.
    if (client === null) {
      dGrpc("streamSessionTerminalIO skip open — null client (transport blip), terminal stays mounted");
      return;
    }
    setInputError(null);
    const refusals = new InputRefusalTracker(setInputError);

    const outputListeners: Array<(frame: TerminalFrame) => void> = [];
    // Frames that arrived before anything subscribed.
    //
    // This effect publishes the stream with `setStream` and starts consuming in the same pass, but
    // the subscriber — `GhosttyTerminalSession`'s own effect — cannot run until React has
    // re-rendered with a non-null `feed`. A frame delivered in that window used to reach an empty
    // listener list and be dropped silently: no screen in the terminal, and no offset recorded.
    //
    // A network round trip normally hides the gap. A fast transport does not: Tddy Desktop hosts
    // the daemon *in this process*, so the first frame can and does beat a React commit.
    const framesBeforeAnyListener: TerminalFrame[] = [];
    let closed = false;
    // The request half: queued input waits here for the iterator below to yield it.
    const outbound: QueuedInput[] = [];
    let wakeInput: (() => void) | null = null;
    const wake = () => wakeInput?.();
    // Set when a frame did not continue the screen: the request half sends one `requestSnapshot`
    // message on this same stream, and the guard drops every delta until the snapshot lands.
    let snapshotWanted = false;
    // A request is outstanding until a snapshot arrives, so a run of gapped frames asks once.
    let snapshotAsked = false;
    const continuity = new TerminalFrameContinuity();

    const terminalStream: TerminalStream = {
      send(data: Uint8Array) {
        if (isResizeOsc(data)) lastResizeRef.current = data;
        sendInputRequestRef.current(data);
      },
      onMessage(fn: (frame: TerminalFrame) => void) {
        outputListeners.push(fn);
        // Hand the new subscriber whatever arrived before it existed, in order. Drained rather
        // than replayed to every later subscriber: this is delivery that was owed, not history.
        framesBeforeAnyListener.splice(0).forEach((frame) => fn(frame));
      },
      close() {
        closed = true;
        wake();
      },
    };
    setStream(terminalStream);
    openInputRef.current = (message) => {
      outbound.push(message);
      wake();
    };

    // Measure container dimensions so the daemon resizes the PTY before it snapshots the screen,
    // which then arrives serialized at this pane's own grid — no 220-column re-wrap on connect.
    const { widthPx, heightPx, cols: initialCols, rows: initialRows } = measureTerminalGridFromRect(
      containerRef.current?.getBoundingClientRect(),
    );
    dResize(
      "streamSessionTerminalIO open sessionId=%s mode=SNAPSHOT container=%dx%dpx initialCols=%d initialRows=%d",
      sessionId,
      widthPx,
      heightPx,
      initialCols,
      initialRows,
    );
    if (initialCols === 0 || initialRows === 0) {
      dResize(
        "streamSessionTerminalIO warning sessionId=%s container not laid out yet (cols=%d rows=%d)",
        sessionId,
        initialCols,
        initialRows,
      );
    }

    // The opening message asks for the screen as it is now, whatever this terminal showed before:
    // a re-open is a fresh VT (or one that missed output while the transport was down), so a byte
    // resume would paint into a screen it cannot be sure it has. The daemon honours the grid only
    // for the screen driving the session; a watcher opens read-only and resizes the PTY once its
    // claim lands, through the resize OSC queued until then.
    async function* requests(): AsyncGenerator<MessageInitShape<typeof SessionTerminalInputSchema>> {
      yield {
        sessionToken,
        sessionId,
        terminalId,
        initialCols,
        initialRows,
        mode: StreamReplayMode.SNAPSHOT,
        controlToken: connectedRef.current?.controlToken ?? "",
      };
      while (!closed) {
        if (snapshotWanted) {
          snapshotWanted = false;
          // No data: asking is a read, so it needs no lease and is never refused as a write.
          yield {
            sessionToken,
            sessionId,
            terminalId,
            requestSnapshot: true,
            controlToken: connectedRef.current?.controlToken ?? "",
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
          sessionToken,
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
        for await (const output of client.streamSessionTerminalIO(
          requests(),
        ) as AsyncIterable<SessionTerminalOutput>) {
          if (closed) break;
          // Identity guard: the daemon stamps every frame with the session and (resolved) terminal
          // it came from. A frame from anywhere else is dropped here, before it can reach this
          // pane's terminal — the stream stays open, since a mis-routed frame says nothing about
          // the frames that follow it.
          if (!isFrameForTerminal(output, { sessionId, terminalId })) {
            dGrpc(
              "dropping foreign terminal frame — frame=%s/%s pane=%s/%s bytes=%d",
              output.sessionId,
              output.terminalId,
              sessionId,
              terminalId,
              output.data.length,
            );
            continue;
          }
          // A refusal frame names an input chunk the daemon dropped; it carries no output.
          if (refusals.observe(output)) continue;
          // ACK frames (empty data, non-zero offset) collapse the enqueued-input overlay.
          if (output.ackedInputOffset > 0n) {
            ackRef.current(Number(output.ackedInputOffset));
          }
          // Forward the full frame (data + offset metadata) to the shared terminal, which captures
          // the lazy-history anchor from the snapshot frame and drives the forward fill of the
          // older-history terminal on demand.
          if (output.data.length > 0 || output.endOffset > 0n) {
            // A frame that does not continue what is painted is dropped, never drawn over a screen
            // it was not computed against; the first one asks for a fresh snapshot, and the
            // daemon's answer re-anchors the guard.
            if (continuity.accept(output) === "gap") {
              if (!snapshotAsked) {
                snapshotAsked = true;
                snapshotWanted = true;
                wake();
              }
              dGrpc("gap — dropping frame and asking for a snapshot start=%s end=%s", output.startOffset, output.endOffset);
              continue;
            }
            if (output.snapshot) snapshotAsked = false;
            const frame: TerminalFrame = {
              data: output.data,
              endOffset: output.endOffset,
              atOldest: output.atOldest,
              snapshot: output.snapshot,
              startOffset: output.startOffset,
            };
            if (outputListeners.length === 0) {
              framesBeforeAnyListener.push(frame);
            } else {
              outputListeners.forEach((fn) => fn(frame));
            }
          }
        }
        if (!closed) emitDisconnect();
      } catch (err) {
        if (!closed) {
          if (clientRef.current !== null) setInputError(rpcFailureText(err));
          emitDisconnect();
        }
      }
    })();

    return () => {
      closed = true;
      wake();
      openInputRef.current = null;
    };
  }, [client, sessionId, terminalId]);

  // Release input that was queued while the claim was in flight (or the client was null) once both
  // `connected` and a non-null `client` arrive. Keyed on `connected` and `client` so it fires when
  // either transitions into a usable state.
  //
  // Gaining the lease (a first claim, or a re-claim after being displaced) also re-asserts this
  // screen's grid when the flush does not already carry it.
  const controlToken = connected?.controlToken ?? null;
  useEffect(() => {
    if (controlToken === null || client === null) return;
    const pending = pendingInputRef.current;
    pendingInputRef.current = [];
    const lastResize = lastResizeRef.current;
    if (lastResize !== null && !pending.includes(lastResize)) {
      dGrpc("re-asserting this screen's grid after gaining control");
      pending.push(lastResize);
    }
    if (pending.length === 0) return;
    dGrpc("flushing %d buffered input chunk(s) after ConnectedSession + client ready", pending.length);
    for (const data of pending) {
      sendInputRequest(data);
    }
  }, [controlToken, client, sendInputRequest]);

  // Tear down the attachment when the terminal unmounts (session switch / screen
  // close). Empty deps so the cleanup runs only on real unmount, not on prop changes.
  useEffect(() => {
    return () => {
      emitDisconnect();
    };
  }, []);

  // Always render the outer div so containerRef.current is available when the
  // effect above runs (before stream is set). Terminal renders once stream is ready.
  const historyFetcher = useMemo<TerminalHistoryFetcher>(() => {
    if (client === null) {
      // No client during a transient transport blip — no history fetch until it returns.
      return async () => null;
    }
    return createForwardHistoryFetcher(client, { sessionToken, sessionId, terminalId });
  }, [client, sessionToken, sessionId, terminalId]);

  // What this pane hands the one terminal component. The daemon can always replay, so history is
  // always offered — which is the answer `feedSupportsHistory` gives the terminal's scrollback.
  //
  // Its identity is the stream's, and the fetcher is reached through a ref: the terminal tears its
  // listener down and closes the stream when the feed changes, so a feed rebuilt merely because a
  // fresh client produced a fresh fetcher would close a stream that is still running.
  const historyFetcherRef = useRef(historyFetcher);
  historyFetcherRef.current = historyFetcher;
  const feed = useMemo<TerminalFeed | null>(
    () =>
      stream
        ? { stream, history: (from, until) => historyFetcherRef.current(from, until) }
        : null,
    [stream],
  );

  return (
    <div ref={containerRef} style={{ width: "100%", height: "100%", position: "relative" }}>
      {feed && (
        <GhosttyTerminalSession
          feed={feed}
          sessionToken={sessionToken}
          sessionId={sessionId}
          mobileShortcuts={mobileShortcuts}
          onRegisterInsertInput={onRegisterInsertInput}
          readsLocalPaths={readsLocalPaths}
        />
      )}
      <EnqueuedInputOverlay model={enqueuedModel} visible={enqueuedVisible} />
      <InlineError
        testId="terminal-input-error"
        message={inputError && `Input not delivered: ${inputError}`}
        className="pointer-events-none absolute bottom-2 left-2 z-10 max-w-[90%] rounded border border-destructive/40 bg-background/90 px-2 py-1 text-xs"
      />
    </div>
  );
}
