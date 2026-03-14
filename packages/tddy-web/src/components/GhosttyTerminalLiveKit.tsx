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

export interface GhosttyTerminalLiveKitProps {
  url: string;
  token: string;
  roomName?: string;
  showBufferTextForTest?: boolean;
  /** When true, skip Ghostty rendering and log RPC received data to console; expose count for test assertion. */
  debugMode?: boolean;
}

export function GhosttyTerminalLiveKit({
  url,
  token,
  roomName = "terminal-e2e",
  showBufferTextForTest = false,
  debugMode = false,
}: GhosttyTerminalLiveKitProps) {
  const termRef = useRef<GhosttyTerminalHandle>(null);
  const inputQueueRef = useRef<Uint8Array[]>([]);
  const outputBufferRef = useRef<Uint8Array[]>([]);
  const streamedTextRef = useRef("");
  const termReadyRef = useRef(false);
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

    const run = async () => {
      try {
        room = new Room();

        room.on(RoomEvent.Connected, () => console.log("[LiveKit] RoomEvent.Connected"));
        room.on(RoomEvent.Disconnected, (_, reason) =>
          console.log("[LiveKit] RoomEvent.Disconnected", reason)
        );
        room.on(RoomEvent.ConnectionStateChanged, (state) =>
          console.log("[LiveKit] ConnectionStateChanged", state)
        );
        room.on(RoomEvent.ParticipantConnected, (participant) =>
          console.log("[LiveKit] ParticipantConnected", participant.identity)
        );
        room.on(RoomEvent.ParticipantDisconnected, (participant) =>
          console.log("[LiveKit] ParticipantDisconnected", participant.identity)
        );

        await room.connect(url, token);

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
      if (bufferInterval) clearInterval(bufferInterval);
      if (room) room.disconnect();
    };
  }, [url, token, roomName, showBufferTextForTest, debugMode]);

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
