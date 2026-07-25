import React, { useCallback, useEffect, useRef, useState } from "react";
import type { Client } from "@connectrpc/connect";
import type { ConnectionService, SessionTerminalOutput } from "../../gen/connection_pb";
import { GhosttyTerminalGrpc, type GrpcStream } from "../GhosttyTerminalGrpc";
import type { ConnectedSession } from "./useTerminalControl";
import type { ToolShortcutDef } from "../../lib/toolShortcuts";
import { tddyDebug } from "../../lib/debugMask";
import { measureTerminalGridFromRect } from "../../lib/terminalGridMeasure";
import { useEnqueuedInput } from "./useEnqueuedInput";
import { EnqueuedInputOverlay } from "../connection/EnqueuedInputOverlay";

const dGrpc = tddyDebug("tddy:term:grpc");
const dResize = tddyDebug("tddy:term:resize");

type ConnectionClient = Client<typeof ConnectionService>;

interface GrpcSessionTerminalProps {
  sessionId: string;
  sessionToken: string;
  client: ConnectionClient;
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
  // Input sent before the claim resolves is queued here and flushed once `connected` arrives —
  // structurally, `sendTerminalInput` cannot go out before the lease exists.
  const pendingInputRef = useRef<Uint8Array[]>([]);

  // Enqueued-input accounting: assigns each sent chunk a cumulative byte offset, consumes the
  // daemon's acked_input_offset, and drives the un-acknowledged-input overlay on slow links.
  const { enqueue, ack, model: enqueuedModel, visible: enqueuedVisible } = useEnqueuedInput();

  // Disconnect fires automatically when the terminal goes away — either the remote
  // stream ends or this component unmounts (e.g. the user switches sessions). Read
  // the latest callback via a ref and guard so it fires at most once.
  const onDisconnectRef = useRef(onDisconnect);
  onDisconnectRef.current = onDisconnect;
  const disconnectedRef = useRef(false);
  const emitDisconnect = () => {
    if (disconnectedRef.current) return;
    disconnectedRef.current = true;
    onDisconnectRef.current?.();
  };

  const sendInputRequest = useCallback(
    (data: Uint8Array) => {
      const conn = connectedRef.current;
      if (conn === null) {
        // No lease yet — the auto-claim is in flight. Queue the input (typically the onReady
        // resize OSC) until `connected` arrives and the flush effect below releases it.
        pendingInputRef.current.push(data);
        return;
      }
      // Assign this chunk its cumulative byte offset (also tracked for the enqueued-input overlay)
      // and send it so the daemon can ack the applied offset back on the output stream.
      const inputOffset = enqueue(data);
      client
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

  useEffect(() => {
    const outputListeners: Array<(data: Uint8Array) => void> = [];
    let closed = false;

    const grpcStream: GrpcStream = {
      send(data: Uint8Array) {
        sendInputRequest(data);
      },
      onMessage(fn: (data: Uint8Array) => void) {
        outputListeners.push(fn);
      },
      close() {
        closed = true;
      },
    };
    setStream(grpcStream);

    // Measure container dimensions so the daemon can resize the PTY before
    // replaying buffered output — eliminates the 220-col garbling on connect.
    const { widthPx, heightPx, cols: initialCols, rows: initialRows } = measureTerminalGridFromRect(
      containerRef.current?.getBoundingClientRect(),
    );
    dResize(
      "streamTerminalOutput open sessionId=%s container=%gx%gpx initialCols=%d initialRows=%d",
      sessionId,
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
        }) as AsyncIterable<SessionTerminalOutput>) {
          if (closed) break;
          // ACK frames (empty data, non-zero offset) collapse the enqueued-input overlay.
          if (output.ackedInputOffset > 0n) {
            ack(Number(output.ackedInputOffset));
          }
          if (output.data.length > 0) {
            outputListeners.forEach((fn) => fn(output.data));
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
  }, [client, sessionId, sessionToken, terminalId, sendInputRequest, ack]);

  // Release input that was queued while the claim was in flight once `connected` arrives. Keyed on
  // `connected` so it fires exactly when the lease transitions null → ConnectedSession.
  useEffect(() => {
    if (connected === null) return;
    const pending = pendingInputRef.current;
    if (pending.length === 0) return;
    pendingInputRef.current = [];
    dGrpc("flushing %d buffered input chunk(s) after ConnectedSession claimed", pending.length);
    for (const data of pending) {
      sendInputRequest(data);
    }
  }, [connected, sendInputRequest]);

  // Tear down the attachment when the terminal unmounts (session switch / screen
  // close). Empty deps so the cleanup runs only on real unmount, not on prop changes.
  useEffect(() => {
    return () => {
      emitDisconnect();
    };
  }, []);

  // Always render the outer div so containerRef.current is available when the
  // effect above runs (before stream is set). Terminal renders once stream is ready.
  return (
    <div ref={containerRef} style={{ width: "100%", height: "100%", position: "relative" }}>
      {stream && (
        <GhosttyTerminalGrpc
          sessionToken={sessionToken}
          sessionId={sessionId}
          stream={stream}
          mobileShortcuts={mobileShortcuts}
          onRegisterInsertInput={onRegisterInsertInput}
        />
      )}
      <EnqueuedInputOverlay model={enqueuedModel} visible={enqueuedVisible} />
    </div>
  );
}
