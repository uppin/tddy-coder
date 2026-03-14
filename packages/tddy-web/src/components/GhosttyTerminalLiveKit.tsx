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
}

export function GhosttyTerminalLiveKit({
  url,
  token,
  getToken,
  ttlSeconds: ttlSecondsProp,
  roomName = "terminal-e2e",
  showBufferTextForTest = false,
  debugMode = false,
}: GhosttyTerminalLiveKitProps) {
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

        room.on(RoomEvent.Connected, () => console.log("[LiveKit] RoomEvent.Connected"));
        room.on(RoomEvent.Disconnected, async (reason) => {
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

        await room.connect(url, initialToken);

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

        const transport = createLiveKitTransport({
          room: room!,
          targetIdentity: SERVER_IDENTITY,
          debug: debugMode,
        });

        const client = createClient(TerminalService, transport);

        const queue = inputQueueRef.current;
        async function* inputGen() {
          yield create(TerminalInputSchema, { data: new Uint8Array(0) });
          while (true) {
            if (queue.length > 0) {
              const data = queue.shift()!;
              yield create(TerminalInputSchema, { data });
            } else {
              await new Promise((r) => setTimeout(r, 100));
            }
          }
        }

        const stream = client.streamTerminalIO(inputGen());

        (async () => {
          try {
            let count = 0;
            let sample = "";
            for await (const output of stream) {
              if (output.data.length === 0) continue;
              count++;
              if (debugMode) {
                console.log("[GhosttyTerminalLiveKit] RPC chunk #", count, "bytes:", output.data.length, "sample:", new TextDecoder().decode(output.data.slice(0, 80)));
                sample = new TextDecoder().decode(output.data.slice(0, 200));
                setRpcReceivedCount(count);
                setRpcReceivedSample(sample);
              }
              if (!debugMode) {
                const decoded = new TextDecoder().decode(output.data);
                streamedTextRef.current += decoded;
                if (termReadyRef.current && termRef.current) {
                  termRef.current.write(output.data);
                } else {
                  outputBufferRef.current.push(output.data);
                }
                if (showBufferTextForTest) {
                  setBufferText(streamedTextRef.current);
                  setStreamedByteCount((c) => c + output.data.length);
                }
              }
            }
          } catch (e) {
            console.error("Stream error:", e);
          }
        })();

        setStatus("connected");

        if (showBufferTextForTest && !debugMode) {
          bufferInterval = setInterval(() => {
            const streamed = streamedTextRef.current;
            const fromBuffer = termRef.current?.getBufferText?.() ?? "";
            const text = streamed || fromBuffer;
            setBufferText(text);
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
  }, [url, token, getToken, ttlSecondsProp, roomName, showBufferTextForTest, debugMode]);

  return (
    <div>
      <div data-testid="livekit-status">{status}</div>
      {status === "error" && (
        <div data-testid="livekit-error">{errorMsg}</div>
      )}
      <div style={{ height: 300 }}>
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
            onReady={() => {
              termReadyRef.current = true;
              const buf = outputBufferRef.current;
              outputBufferRef.current = [];
              const term = termRef.current;
              if (term) {
                for (const chunk of buf) {
                  term.write(chunk);
                }
              }
            }}
            onData={(data) => {
              const encoder = new TextEncoder();
              inputQueueRef.current.push(encoder.encode(data));
            }}
          />
        )}
      </div>
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
        </>
      )}
    </div>
  );
}
