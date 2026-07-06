import React, { useEffect, useRef, useState } from "react";
import type { Client } from "@connectrpc/connect";
import type { ConnectionService, SessionTerminalOutput } from "../../gen/connection_pb";
import { GhosttyTerminalGrpc, type GrpcStream } from "../GhosttyTerminalGrpc";
import type { ToolShortcutDef } from "../../lib/toolShortcuts";
import { tddyDebug } from "../../lib/debugMask";

const dGrpc = tddyDebug("tddy:term:grpc");

type ConnectionClient = Client<typeof ConnectionService>;

// Character cell size estimates for converting container pixels to terminal columns/rows.
// Based on a monospace font rendered at the default terminal font size (14px).
const CHAR_WIDTH_PX = 8;
const CHAR_HEIGHT_PX = 17;

interface GrpcSessionTerminalProps {
  sessionId: string;
  sessionToken: string;
  client: ConnectionClient;
  controlToken?: string;
  onDisconnect?: () => void;
  mobileShortcuts?: ToolShortcutDef[];
}

export function GrpcSessionTerminal({
  sessionId,
  sessionToken,
  client,
  controlToken,
  onDisconnect,
  mobileShortcuts,
}: GrpcSessionTerminalProps) {
  const [stream, setStream] = useState<GrpcStream | null>(null);
  // containerRef must be on a div that is ALWAYS rendered (not gated on stream),
  // so getBoundingClientRect() returns real dimensions when the effect runs.
  const containerRef = useRef<HTMLDivElement>(null);
  // Keep a ref so the send() closure always reads the latest token without
  // needing to recreate the grpcStream (and remount the terminal) on each change.
  const controlTokenRef = useRef<string>(controlToken ?? "");
  controlTokenRef.current = controlToken ?? "";

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

  useEffect(() => {
    const outputListeners: Array<(data: Uint8Array) => void> = [];
    let closed = false;

    const grpcStream: GrpcStream = {
      send(data: Uint8Array) {
        client
          .sendTerminalInput({ sessionToken, sessionId, data, controlToken: controlTokenRef.current })
          .catch((err) => {
            dGrpc(
              "sendTerminalInput failed sessionId=%s error=%o",
              sessionId,
              err instanceof Error ? err.message : err,
            );
          });
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
    const rect = containerRef.current?.getBoundingClientRect();
    const initialCols = rect && rect.width > 0 ? Math.max(1, Math.floor(rect.width / CHAR_WIDTH_PX)) : 0;
    const initialRows = rect && rect.height > 0 ? Math.max(1, Math.floor(rect.height / CHAR_HEIGHT_PX)) : 0;
    void (async () => {
      try {
        for await (const output of client.streamTerminalOutput({
          sessionToken,
          sessionId,
          initialCols,
          initialRows,
        }) as AsyncIterable<SessionTerminalOutput>) {
          if (closed) break;
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
  }, [client, sessionId, sessionToken]);

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
    <div ref={containerRef} style={{ width: "100%", height: "100%" }}>
      {stream && (
        <GhosttyTerminalGrpc
          sessionToken={sessionToken}
          sessionId={sessionId}
          stream={stream}
          mobileShortcuts={mobileShortcuts}
        />
      )}
    </div>
  );
}
