import React, { useCallback, useEffect, useMemo, useRef, useState } from "react";
import type { Client } from "@connectrpc/connect";
import {
  ConnectionService,
  SessionTerminalOutput,
  StreamReplayMode,
} from "../../gen/connection_pb";
import { GhosttyTerminalGrpc, type GrpcFrame, type GrpcStream, type HistoryFetcher } from "../GhosttyTerminalGrpc";
import type { ConnectedSession } from "./useTerminalControl";
import type { ToolShortcutDef } from "../../lib/toolShortcuts";
import { tddyDebug } from "../../lib/debugMask";
import { measureTerminalGridFromRect } from "../../lib/terminalGridMeasure";
import { useEnqueuedInput } from "./useEnqueuedInput";
import { EnqueuedInputOverlay } from "../connection/EnqueuedInputOverlay";
import { createForwardHistoryFetcher } from "../../lib/terminalHistoryLoader";

const dGrpc = tddyDebug("tddy:term:grpc");
const dResize = tddyDebug("tddy:term:resize");

type ConnectionClient = Client<typeof ConnectionService>;

interface GrpcSessionTerminalProps {
  sessionId: string;
  sessionToken: string;
  /** The daemon ConnectionService client. `null` while the transport is in a transient blip — the
   *  terminal stays mounted (its scrollback and the ghostty instance survive) and resumes with
   *  `FROM_OFFSET` when a non-null client returns. Only a stream-end with a *valid* client (real
   *  `pty_done`) evicts the runtime. */
  client: ConnectionClient | null;
  /** The claimed session (lease token in hand), or `null` while the auto-claim is in flight / was
   *  denied. `sendTerminalInput` is gated on this: input sent before the claim resolves (typically
   *  the onReady resize OSC) is queued and flushed once the claim arrives, so it can never go out
   *  with an empty/stale token. The output stream (`streamTerminalOutput`) needs no token and opens
   *  immediately. */
  connected: ConnectedSession | null;
  /** Target terminal within the session. Empty (the default) resolves to the reserved main
   *  ("claude"/Agent) terminal on the daemon; a bash terminal passes its own `terminal_id`. */
  terminalId?: string;
  onDisconnect?: () => void;
  mobileShortcuts?: ToolShortcutDef[];
  /** Called with this terminal's text-insert function (see `GhosttyTerminalGrpc.onRegisterInsertInput`),
   *  so the runtime can expose it to the inspector's Files-tab click/tap route. */
  onRegisterInsertInput?: (insertInput: (text: string) => void) => void;
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
}: GrpcSessionTerminalProps) {
  const [stream, setStream] = useState<GrpcStream | null>(null);
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
  // and resumes with `FROM_OFFSET` when a non-null client returns.
  const clientRef = useRef<ConnectionClient | null>(client);
  clientRef.current = client;
  const sessionTokenRef = useRef(sessionToken);
  sessionTokenRef.current = sessionToken;
  // Input sent before the claim resolves (or while the client is null on a transport blip) is
  // queued here and flushed once `connected` and a non-null client both arrive.
  const pendingInputRef = useRef<Uint8Array[]>([]);

  // Replay-once / resume-by-offset tracking. `currentOffset` is the cumulative output byte offset
  // the client has received up to (snapped to the frame's absolute `endOffset` on replay/catch-up
  // frames, advanced by byte length on live tail frames). `hasSynced` records that the initial
  // TAIL replay has landed, so a subsequent stream open (reconnect) sends `FROM_OFFSET` with the
  // tracked offset instead of re-replaying — no duplicate content in the live terminal.
  const currentOffsetRef = useRef(0n);
  const hasSyncedRef = useRef(false);

  // Enqueued-input accounting: assigns each sent chunk a cumulative byte offset, consumes the
  // daemon's acked_input_offset, and drives the un-acknowledged-input overlay on slow links.
  const { enqueue, ack, model: enqueuedModel, visible: enqueuedVisible } = useEnqueuedInput();
  const ackRef = useRef(ack);
  ackRef.current = ack;

  // Disconnect fires automatically when the terminal goes away — either the remote
  // stream ends or this component unmounts (e.g. the user switches sessions). Read
  // the latest callback via a ref and guard so it fires at most once. CRITICAL: a stream-end with
  // a *null* client (transient transport blip) must NOT evict — the terminal pauses and waits for
  // a non-null client to return, then resumes with `FROM_OFFSET`. Only a stream-end with a valid
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
      const cli = clientRef.current;
      if (conn === null || cli === null) {
        // No lease yet (auto-claim in flight) OR no client (transient transport blip). Queue the
        // input (typically the onReady resize OSC) until both arrive and the flush effect below
        // releases it.
        pendingInputRef.current.push(data);
        return;
      }
      // Assign this chunk its cumulative byte offset (also tracked for the enqueued-input overlay)
      // and send it so the daemon can ack the applied offset back on the output stream.
      const inputOffset = enqueue(data);
      cli
        .sendTerminalInput({
          sessionToken,
          sessionId,
          terminalId,
          data,
          controlToken: conn.controlToken,
          inputOffset: BigInt(inputOffset),
        })
        .catch((err) => {
          dGrpc(
            "sendTerminalInput failed sessionId=%s error=%o",
            sessionId,
            err instanceof Error ? err.message : err,
          );
        });
    },
    [client, sessionId, sessionToken, terminalId, enqueue],
  );
  const sendInputRequestRef = useRef(sendInputRequest);
  sendInputRequestRef.current = sendInputRequest;

  useEffect(() => {
    // No client during a transient transport blip — keep the previous stream (and the ghostty
    // terminal instance) mounted; resume with `FROM_OFFSET` when a non-null client returns.
    if (client === null) {
      dGrpc("streamTerminalOutput skip open — null client (transport blip), terminal stays mounted");
      return;
    }

    const outputListeners: Array<(frame: GrpcFrame) => void> = [];
    let closed = false;

    const grpcStream: GrpcStream = {
      send(data: Uint8Array) {
        sendInputRequestRef.current(data);
      },
      onMessage(fn: (frame: GrpcFrame) => void) {
        outputListeners.push(fn);
      },
      close() {
        closed = true;
      },
    };
    setStream(grpcStream);

    // Measure container dimensions so the daemon can resize the PTY before replaying buffered
    // output — eliminates the 220-col garbling on connect. Only sent on the first (TAIL) open;
    // a FROM_OFFSET reconnect reuses the live terminal's existing dimensions (no resize/drain).
    const { widthPx, heightPx, cols: initialCols, rows: initialRows } = measureTerminalGridFromRect(
      containerRef.current?.getBoundingClientRect(),
    );
    const mode = hasSyncedRef.current ? StreamReplayMode.FROM_OFFSET : StreamReplayMode.TAIL;
    const fromOffset = hasSyncedRef.current ? currentOffsetRef.current : 0n;
    dResize(
      "streamTerminalOutput open sessionId=%s mode=%s fromOffset=%s container=%gx%gpx initialCols=%d initialRows=%d",
      sessionId,
      hasSyncedRef.current ? "FROM_OFFSET" : "TAIL",
      fromOffset.toString(),
      widthPx,
      heightPx,
      initialCols,
      initialRows,
    );
    if (initialCols === 0 || initialRows === 0) {
      dResize(
        "streamTerminalOutput warning sessionId=%s container not laid out yet (cols=%d rows=%d)",
        sessionId,
        initialCols,
        initialRows,
      );
    }
    void (async () => {
      try {
        for await (const output of client.streamTerminalOutput({
          sessionToken,
          sessionId,
          terminalId,
          initialCols,
          initialRows,
          mode,
          fromOffset,
        }) as AsyncIterable<SessionTerminalOutput>) {
          if (closed) break;
          // ACK frames (empty data, non-zero offset) collapse the enqueued-input overlay.
          if (output.ackedInputOffset > 0n) {
            ackRef.current(Number(output.ackedInputOffset));
          }
          // Forward the full frame (data + offset metadata) to the shared terminal, which captures
          // the lazy-history anchor from the initial replay frame and drives the forward fill of
          // the older-history terminal on demand.
          if (output.data.length > 0 || output.endOffset > 0n) {
            const frame: GrpcFrame = {
              data: output.data,
              endOffset: output.endOffset,
              atOldest: output.atOldest,
            };
            outputListeners.forEach((fn) => fn(frame));
          }
        }
        if (!closed) emitDisconnect();
      } catch (err) {
        if (!closed) emitDisconnect();
      }
    })();

    return () => {
      closed = true;
    };
  }, [client, sessionId, terminalId]);

  // Release input that was queued while the claim was in flight (or the client was null) once both
  // `connected` and a non-null `client` arrive. Keyed on `connected` and `client` so it fires when
  // either transitions into a usable state.
  useEffect(() => {
    if (connected === null || client === null) return;
    const pending = pendingInputRef.current;
    if (pending.length === 0) return;
    pendingInputRef.current = [];
    dGrpc("flushing %d buffered input chunk(s) after ConnectedSession + client ready", pending.length);
    for (const data of pending) {
      sendInputRequest(data);
    }
  }, [connected, client, sendInputRequest]);

  // Tear down the attachment when the terminal unmounts (session switch / screen
  // close). Empty deps so the cleanup runs only on real unmount, not on prop changes.
  useEffect(() => {
    return () => {
      emitDisconnect();
    };
  }, []);

  // Always render the outer div so containerRef.current is available when the
  // effect above runs (before stream is set). Terminal renders once stream is ready.
  const historyFetcher = useMemo<HistoryFetcher>(() => {
    if (client === null) {
      // No client during a transient transport blip — no history fetch until it returns.
      return async () => null;
    }
    return createForwardHistoryFetcher(client, { sessionToken, sessionId, terminalId });
  }, [client, sessionToken, sessionId, terminalId]);

  return (
    <div ref={containerRef} style={{ width: "100%", height: "100%", position: "relative" }}>
      {stream && (
        <GhosttyTerminalGrpc
          sessionToken={sessionToken}
          sessionId={sessionId}
          stream={stream}
          mobileShortcuts={mobileShortcuts}
          onRegisterInsertInput={onRegisterInsertInput}
          historyFetcher={historyFetcher}
          onOffsetUpdate={(offset) => {
            currentOffsetRef.current = offset;
            hasSyncedRef.current = true;
          }}
        />
      )}
      <EnqueuedInputOverlay model={enqueuedModel} visible={enqueuedVisible} />
    </div>
  );
}
