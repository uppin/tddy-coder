import React, { useEffect, useRef, useState } from "react";
import { Room, RoomEvent } from "livekit-client";
import { createClient } from "@connectrpc/connect";
import {
  createLiveKitTransport,
  TerminalService,
  TerminalInputSchema,
} from "tddy-livekit-web";
import { create } from "@bufbuild/protobuf";
import { GhosttyTerminal, type GhosttyTerminalHandle } from "./GhosttyTerminal";

const SERVER_IDENTITY = "server";

/** Human-readable description of a terminal input byte sequence. */
function describeKey(bytes: Uint8Array): string {
  if (bytes.length === 1) {
    const b = bytes[0];
    if (b === 0x0d) return "Enter";
    if (b === 0x1b) return "Esc";
    if (b === 0x09) return "Tab";
    if (b === 0x7f) return "Backspace";
    if (b === 0x03) return "Ctrl+C";
    if (b < 0x20) return `Ctrl+${String.fromCharCode(b + 0x40)}`;
    if (b >= 0x20 && b < 0x7f) return `'${String.fromCharCode(b)}'`;
  }
  if (bytes.length === 3 && bytes[0] === 0x1b && bytes[1] === 0x5b) {
    const c = bytes[2];
    if (c === 0x41) return "Up";
    if (c === 0x42) return "Down";
    if (c === 0x43) return "Right";
    if (c === 0x44) return "Left";
  }
  if (bytes.length === 4 && bytes[0] === 0x1b && bytes[1] === 0x5b && bytes[3] === 0x7e) {
    if (bytes[2] === 0x35) return "PageUp";
    if (bytes[2] === 0x36) return "PageDown";
  }
  return `raw(${bytes.length})`;
}

export interface TokenResult {
  token: string;
  ttlSeconds: bigint;
}

export interface GhosttyTerminalLiveKitProps {
  url: string;
  /** Pre-generated JWT token. When both token and getToken provided, used for initial connect; getToken used for refresh. */
  token?: string;
  /** Callback to fetch/refresh token from Connect-RPC. When provided with token, used for refresh only; otherwise for initial + refresh. */
  getToken?: () => Promise<TokenResult>;
  /** TTL in seconds for refresh schedule. Required when both token and getToken provided. */
  ttlSeconds?: bigint;
  roomName?: string;
  showBufferTextForTest?: boolean;
  /** When true, skip Ghostty rendering and log RPC received data to console; expose count for test assertion. */
  debugMode?: boolean;
  /** When true, log data flows and lifecycle events to console for debugging. */
  debugLogging?: boolean;
}

export function GhosttyTerminalLiveKit({
  url,
  token,
  getToken,
  ttlSeconds: ttlSecondsProp,
  roomName = "terminal-e2e",
  showBufferTextForTest = false,
  debugMode = false,
  debugLogging = false,
}: GhosttyTerminalLiveKitProps) {
  const log = debugLogging
    ? (...args: unknown[]) => console.log("[GhosttyLiveKit]", ...args)
    : () => {};
  const termRef = useRef<GhosttyTerminalHandle>(null);
  const inputQueueRef = useRef<Uint8Array[]>([]);
  const outputBufferRef = useRef<Uint8Array[]>([]);
  const streamedTextRef = useRef("");
  const termReadyRef = useRef(false);
  const latestTokenRef = useRef<string | null>(null);
  const refreshTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const [status, setStatus] = useState<"connecting" | "connected" | "error">(
    "connecting"
  );
  const [bufferText, setBufferText] = useState("");
  const [streamedByteCount, setStreamedByteCount] = useState(0);
  const [errorMsg, setErrorMsg] = useState("");
  const [rpcReceivedCount, setRpcReceivedCount] = useState(0);
  const [rpcReceivedSample, setRpcReceivedSample] = useState("");
  const [firstOutputReceived, setFirstOutputReceived] = useState(false);
  const [highlightedLine, setHighlightedLine] = useState("");

  useEffect(() => {
    let room: Room | null = null;
    let bufferInterval: ReturnType<typeof setInterval> | null = null;
    let cancelled = false;

    const run = async () => {
      try {
        let initialToken: string;
        let ttlSecondsForRefresh: bigint | null = null;
        if (token && getToken && ttlSecondsProp !== undefined) {
          initialToken = token;
          latestTokenRef.current = token;
          ttlSecondsForRefresh = ttlSecondsProp;
        } else if (getToken) {
          const result = await getToken();
          initialToken = result.token;
          latestTokenRef.current = result.token;
          ttlSecondsForRefresh = result.ttlSeconds;
        } else if (token) {
          initialToken = token;
        } else {
          throw new Error("Either token or getToken must be provided");
        }

        if (getToken && ttlSecondsForRefresh !== null) {
          const ttlMs = Number(ttlSecondsForRefresh) * 1000;
          const refreshInMs = Math.max(0, ttlMs - 60 * 1000);
          const scheduleRefresh = (delayMs: number) => {
            refreshTimerRef.current = setTimeout(async () => {
              try {
                const next = await getToken();
                latestTokenRef.current = next.token;
                const nextTtlMs = Number(next.ttlSeconds) * 1000;
                const nextRefreshMs = Math.max(0, nextTtlMs - 60 * 1000);
                scheduleRefresh(nextRefreshMs);
              } catch (e) {
                console.warn("[GhosttyTerminalLiveKit] Token refresh failed:", e);
              }
            }, delayMs);
          };
          scheduleRefresh(refreshInMs);
        }

        room = new Room();

        room.on(RoomEvent.Connected, () => {
          log("lifecycle: RoomEvent.Connected");
          console.log("[LiveKit] RoomEvent.Connected");
        });
        room.on(RoomEvent.Disconnected, async (reason) => {
          log("lifecycle: RoomEvent.Disconnected", reason);
          console.log("[LiveKit] RoomEvent.Disconnected", reason);
          if (!cancelled && getToken && latestTokenRef.current) {
            try {
              await room!.connect(url, latestTokenRef.current);
            } catch (e) {
              console.warn("[GhosttyTerminalLiveKit] Reconnect failed:", e);
            }
          }
        });
        room.on(RoomEvent.ConnectionStateChanged, (state) =>
          console.log("[LiveKit] ConnectionStateChanged", state)
        );
        room.on(RoomEvent.ParticipantConnected, (participant) =>
          console.log("[LiveKit] ParticipantConnected", participant.identity)
        );
        room.on(RoomEvent.ParticipantDisconnected, (participant) =>
          console.log("[LiveKit] ParticipantDisconnected", participant.identity)
        );

        log("lifecycle: connecting to room");
        await room.connect(url, initialToken);
        log("lifecycle: room connected");

        const hasServer = () =>
          Array.from(room!.remoteParticipants.values()).some(
            (p) => p.identity === SERVER_IDENTITY
          );

        await new Promise<void>((resolve, reject) => {
          if (hasServer()) return resolve();
          const t = setTimeout(
            () => reject(new Error("Server participant not found")),
            10000
          );
          const handler = () => {
            if (hasServer()) {
              clearTimeout(t);
              room!.off(RoomEvent.ParticipantConnected, handler);
              resolve();
            }
          };
          room!.on(RoomEvent.ParticipantConnected, handler);
        });
        log("lifecycle: server participant found");

        const transport = createLiveKitTransport({
          room: room!,
          targetIdentity: SERVER_IDENTITY,
          debug: debugMode || debugLogging,
        });
        log("lifecycle: transport created");

        const client = createClient(TerminalService, transport);

        const queue = inputQueueRef.current;
        async function* inputGen() {
          log("dataflow: inputGen started, yielding empty init");
          yield create(TerminalInputSchema, { data: new Uint8Array(0) });
          while (true) {
            if (queue.length > 0) {
              const data = queue.shift()!;
              const preview = new TextDecoder().decode(data.slice(0, 40));
              log("dataflow: inputGen yielding", data.length, "bytes", JSON.stringify(preview));
              yield create(TerminalInputSchema, { data });
            } else {
              await new Promise((r) => setTimeout(r, 100));
            }
          }
        }

        log("lifecycle: starting streamTerminalIO");
        const stream = client.streamTerminalIO(inputGen());

        (async () => {
          try {
            let count = 0;
            let sample = "";
            for await (const output of stream) {
              if (output.data.length === 0) continue;
              count++;
              const preview = new TextDecoder().decode(output.data.slice(0, 60));
              if (debugMode) {
                console.log("[GhosttyTerminalLiveKit] RPC chunk #", count, "bytes:", output.data.length, "sample:", preview);
                sample = new TextDecoder().decode(output.data.slice(0, 200));
                setRpcReceivedCount(count);
                setRpcReceivedSample(sample);
              }
              if (!debugMode) {
                if (count === 1) {
                  setFirstOutputReceived(true);
                  log("dataflow: first RPC output received");
                }
                const decoded = new TextDecoder().decode(output.data);
                streamedTextRef.current += decoded;
                const ready = termReadyRef.current;
                const hasTerm = !!termRef.current;
                log("dataflow: RPC output chunk #", count, "bytes:", output.data.length, "termReady:", ready, "hasTerm:", hasTerm, "preview:", JSON.stringify(preview));
                if (ready && termRef.current) {
                  log("dataflow: writing to term");
                  termRef.current.write(output.data);
                } else {
                  log("dataflow: buffering (outputBuffer length:", outputBufferRef.current.length + 1, ")");
                  outputBufferRef.current.push(output.data);
                }
                if (showBufferTextForTest) {
                  setBufferText(streamedTextRef.current);
                  setStreamedByteCount((c) => c + output.data.length);
                }
              }
            }
            log("lifecycle: stream ended");
          } catch (e) {
            log("lifecycle: stream error", e);
            console.error("Stream error:", e);
          }
        })();

        setStatus("connected");
        log("lifecycle: status=connected");

        if (showBufferTextForTest && !debugMode) {
          bufferInterval = setInterval(() => {
            // Prefer Ghostty's parsed buffer (clean text) over raw streamed ANSI.
            const fromBuffer = termRef.current?.getBufferText?.() ?? "";
            const text = fromBuffer || streamedTextRef.current;
            setBufferText(text);

            const lines = termRef.current?.getBufferLines?.() ?? [];
            const inverseLine = lines.find((l) => l.hasInverse && l.text.trim().length > 0);
            if (inverseLine) {
              setHighlightedLine(inverseLine.text);
            }
          }, 200);
        }
      } catch (e) {
        const err = e instanceof Error ? e : new Error(String(e));
        setErrorMsg(err.message);
        setStatus("error");
      }
    };

    run();

    return () => {
      cancelled = true;
      if (refreshTimerRef.current) clearTimeout(refreshTimerRef.current);
      if (bufferInterval) clearInterval(bufferInterval);
      if (room) room.disconnect();
    };
  }, [url, token, getToken, ttlSecondsProp, roomName, showBufferTextForTest, debugMode, debugLogging]);

  return (
    <div style={{ display: "flex", flexDirection: "column", height: "100%" }}>
      <div data-testid="livekit-status">{status}</div>
      {status === "error" && (
        <div data-testid="livekit-error">{errorMsg}</div>
      )}
      <div style={{ flex: 1, minHeight: 0 }}>
        {debugMode ? (
          <div data-testid="rpc-debug-panel">
            <div data-testid="rpc-received-count">{rpcReceivedCount}</div>
            <div data-testid="rpc-received-sample" style={{ fontSize: 10, fontFamily: "monospace", wordBreak: "break-all" }}>
              {rpcReceivedSample || "(waiting…)"}
            </div>
          </div>
        ) : (
          <GhosttyTerminal
            ref={termRef}
            debugLogging={debugLogging}
            onReady={() => {
              termReadyRef.current = true;
              const buf = outputBufferRef.current;
              outputBufferRef.current = [];
              const term = termRef.current;
              log("lifecycle: onReady, flushing buffer", buf.length, "chunks");
              if (term) {
                for (const chunk of buf) {
                  log("dataflow: onReady flush write", chunk.length, "bytes");
                  term.write(chunk);
                }
              }
            }}
            onData={(data) => {
              const encoded = new TextEncoder().encode(data);
              inputQueueRef.current.push(encoded);
              const keyName = describeKey(encoded);
              console.log("[terminal→server]", keyName, `(${encoded.length} bytes)`, Array.from(encoded));
            }}
          />
        )}
      </div>
      {firstOutputReceived && (
        <div data-testid="first-output-received" style={{ display: "none" }} aria-hidden />
      )}
      {showBufferTextForTest && (
        <>
          <div data-testid="streamed-byte-count" style={{ display: "none" }} aria-hidden>
            {streamedByteCount}
          </div>
          <div
            data-testid="terminal-buffer-text"
            style={{ display: "none" }}
            aria-hidden
          >
            {bufferText}
          </div>
          <div
            data-testid="terminal-highlighted-line"
            style={{ display: "none" }}
            aria-hidden
          >
            {highlightedLine}
          </div>
        </>
      )}
    </div>
  );
}
