import React, { useEffect, useState } from "react";
import {
  Room,
  RoomEvent,
  ConnectionState,
  DataPacket_Kind,
} from "livekit-client";
import { createClient } from "@connectrpc/connect";
import { createLiveKitTransport } from "../../src/index.js";
import { EchoService, EchoRequestSchema, EchoResponseSchema } from "../../src/gen/test/echo_service_pb.js";

const SERVER_IDENTITY = "server";

function logLifecycle(msg: string): void {
  console.log(`[TEST] ${msg}`);
}

function waitForParticipant(room: Room, identity: string, timeoutMs = 10000): Promise<void> {
  const hasServer = () =>
    Array.from(room.remoteParticipants.values()).some((p) => p.identity === identity);
  if (hasServer()) return Promise.resolve();
  return new Promise((resolve, reject) => {
    const t = setTimeout(
      () => reject(new Error(`Participant ${identity} not found within ${timeoutMs}ms`)),
      timeoutMs
    );
    const handler = () => {
      if (hasServer()) {
        clearTimeout(t);
        room.off(RoomEvent.ParticipantConnected, handler);
        logLifecycle(`ParticipantConnected: ${identity}`);
        resolve();
      }
    };
    room.on(RoomEvent.ParticipantConnected, handler);
    if (hasServer()) {
      clearTimeout(t);
      room.off(RoomEvent.ParticipantConnected, handler);
      logLifecycle(`ParticipantConnected: ${identity} (already present)`);
      resolve();
    }
  });
}

/** Wait for RoomEvent.Connected if not already connected. */
function waitForConnected(room: Room): Promise<void> {
  if (room.state === ConnectionState.Connected) {
    logLifecycle("RoomEvent.Connected (already connected)");
    return Promise.resolve();
  }
  return new Promise((resolve) => {
    const handler = () => {
      room.off(RoomEvent.Connected, handler);
      logLifecycle("RoomEvent.Connected");
      resolve();
    };
    room.on(RoomEvent.Connected, handler);
  });
}

/**
 * Wait for data channel to be ready (DCBufferStatusChanged with RELIABLE and isLow=false).
 * Falls back to resolving after timeoutMs if the event never fires.
 */
function waitForDataChannelReady(room: Room, timeoutMs = 5000): Promise<void> {
  return new Promise((resolve) => {
    const t = setTimeout(() => {
      room.off(RoomEvent.DCBufferStatusChanged, handler);
      logLifecycle(`DCBufferStatusChanged: timeout after ${timeoutMs}ms, proceeding`);
      resolve();
    }, timeoutMs);
    const handler = (isLow: boolean, kind: DataPacket_Kind) => {
      logLifecycle(`DCBufferStatusChanged: isLow=${isLow} kind=${kind}`);
      if (kind === DataPacket_Kind.RELIABLE && !isLow) {
        clearTimeout(t);
        room.off(RoomEvent.DCBufferStatusChanged, handler);
        logLifecycle("DCBufferStatusChanged: RELIABLE ready (isLow=false)");
        resolve();
      }
    };
    room.on(RoomEvent.DCBufferStatusChanged, handler);
  });
}

export interface TransportTestHarnessProps {
  url: string;
  token: string;
  action: "unary" | "server-stream" | "client-stream" | "bidi-stream" | "error" | "abort";
  message: string;
}

export function TransportTestHarness({
  url,
  token,
  action,
  message,
}: TransportTestHarnessProps) {
  const [status, setStatus] = useState<"connecting" | "waiting" | "done" | "error">("connecting");
  const [errorMsg, setErrorMsg] = useState<string>("");

  useEffect(() => {
    let room: Room | null = null;

    const run = async () => {
      try {
        room = new Room();

        room.on(RoomEvent.ConnectionStateChanged, (state) => {
          logLifecycle(`ConnectionStateChanged: ${state}`);
        });
        room.on(RoomEvent.Connected, () => logLifecycle("RoomEvent.Connected (listener)"));
        room.on(RoomEvent.Disconnected, () => logLifecycle("RoomEvent.Disconnected"));

        await room.connect(url, token);
        await waitForConnected(room);
        await waitForParticipant(room, SERVER_IDENTITY);
        await waitForDataChannelReady(room);
        setStatus("waiting");

        const targetIdentity = SERVER_IDENTITY;
        const transport = createLiveKitTransport({
          room,
          targetIdentity,
          debug: true,
        });

        const client = createClient(EchoService, transport);

        if (action === "unary") {
          const response = await client.echo({ message });
          console.log(`[TEST] echo result: ${response.message}`);
          setStatus("done");
        } else if (action === "server-stream") {
          const stream = client.echoServerStream({ message });
          let i = 0;
          for await (const msg of stream) {
            console.log(`[TEST] stream message ${i}: ${msg.message}`);
            i++;
          }
          setStatus("done");
        } else if (action === "client-stream") {
          async function* gen() {
            for (const m of message.split(" ")) {
              yield { message: m };
            }
          }
          const response = await client.echoClientStream(gen());
          console.log(`[TEST] echo result: ${response.message}`);
          setStatus("done");
        } else if (action === "bidi-stream") {
          async function* gen() {
            for (const m of message.split(" ")) {
              yield { message: m };
            }
          }
          const stream = client.echoBidiStream(gen());
          let i = 0;
          for await (const msg of stream) {
            console.log(`[TEST] stream message ${i}: ${msg.message}`);
            i++;
          }
          setStatus("done");
        } else if (action === "error") {
          const fakeMethod = {
            parent: { typeName: "unknown.UnknownService" },
            name: "FakeMethod",
            input: EchoRequestSchema,
            output: EchoResponseSchema,
          } as any;
          await transport.unary(fakeMethod, undefined, undefined, undefined, { message: "x" });
          setStatus("done");
        } else if (action === "abort") {
          const controller = new AbortController();
          controller.abort();
          await client.echo({ message }, { signal: controller.signal });
          setStatus("done");
        }
      } catch (e) {
        const err = e instanceof Error ? e : new Error(String(e));
        console.log(`[TEST] error: ${err.message}`);
        setErrorMsg(err.message);
        setStatus("error");
      } finally {
        if (room) {
          room.disconnect();
        }
      }
    };

    run();
  }, [url, token, action, message]);

  return (
    <div>
      <div data-test="status">
        {status === "connecting" && "connecting"}
        {status === "waiting" && "waiting"}
        {status === "done" && "done"}
        {status === "error" && `error: ${errorMsg}`}
      </div>
    </div>
  );
}
