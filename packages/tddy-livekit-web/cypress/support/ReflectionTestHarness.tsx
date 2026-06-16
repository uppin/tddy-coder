import React, { useEffect, useState } from "react";
import {
  Room,
  RoomEvent,
  ConnectionState,
  DataPacket_Kind,
} from "livekit-client";
import { createClient } from "@connectrpc/connect";
import { create, createFileRegistry, fromBinary, toJsonString } from "@bufbuild/protobuf";
import { FileDescriptorSetSchema } from "@bufbuild/protobuf/wkt";
import { createLiveKitTransport } from "../../src/index.js";
import {
  EchoRequestSchema,
  EchoResponseSchema,
} from "../../src/gen/test/echo_service_pb.js";
import {
  ServerReflection,
  ServerReflectionRequestSchema,
} from "../../src/gen/grpc/reflection/v1/reflection_pb.js";

const SERVER_IDENTITY = "server";

function logReflection(msg: string): void {
  console.log(`[REFLECTION] ${msg}`);
}

function waitForParticipant(
  room: Room,
  identity: string,
  timeoutMs = 10000,
): Promise<void> {
  const hasServer = () =>
    Array.from(room.remoteParticipants.values()).some(
      (p) => p.identity === identity,
    );
  if (hasServer()) return Promise.resolve();
  return new Promise((resolve, reject) => {
    const t = setTimeout(
      () =>
        reject(
          new Error(`Participant ${identity} not found within ${timeoutMs}ms`),
        ),
      timeoutMs,
    );
    const handler = () => {
      if (hasServer()) {
        clearTimeout(t);
        room.off(RoomEvent.ParticipantConnected, handler);
        resolve();
      }
    };
    room.on(RoomEvent.ParticipantConnected, handler);
  });
}

function waitForConnected(room: Room): Promise<void> {
  if (room.state === ConnectionState.Connected) return Promise.resolve();
  return new Promise((resolve) => {
    const handler = () => {
      room.off(RoomEvent.Connected, handler);
      resolve();
    };
    room.on(RoomEvent.Connected, handler);
  });
}

function waitForDataChannelReady(room: Room, timeoutMs = 5000): Promise<void> {
  return new Promise((resolve) => {
    const t = setTimeout(() => {
      room.off(RoomEvent.DCBufferStatusChanged, handler);
      resolve();
    }, timeoutMs);
    const handler = (isLow: boolean, kind: DataPacket_Kind) => {
      if (kind === DataPacket_Kind.RELIABLE && !isLow) {
        clearTimeout(t);
        room.off(RoomEvent.DCBufferStatusChanged, handler);
        resolve();
      }
    };
    room.on(RoomEvent.DCBufferStatusChanged, handler);
  });
}

/** Drive the ServerReflection bidi stream with a single request. */
async function* once<T>(value: T): AsyncIterable<T> {
  yield value;
}

/** Encode FileDescriptorProto bytes as a FileDescriptorSet (repeated field #1). */
function encodeFileDescriptorSet(fileProtos: Uint8Array[]): Uint8Array {
  const parts: number[] = [];
  for (const proto of fileProtos) {
    parts.push(0x0a);
    let v = proto.length >>> 0;
    while (v > 0x7f) {
      parts.push((v & 0x7f) | 0x80);
      v >>>= 7;
    }
    parts.push(v);
    for (let i = 0; i < proto.length; i += 1) parts.push(proto[i]);
  }
  return Uint8Array.from(parts);
}

export interface ReflectionTestHarnessProps {
  url: string;
  token: string;
  /** Which reflection scenario to exercise. */
  action:
    | "list-services"
    | "dynamic-invoke-echo"
    | "dynamic-invoke-server-stream"
    | "dynamic-invoke-bidi-stream";
  /** Input message text for invoke actions. */
  message?: string;
}

export function ReflectionTestHarness({
  url,
  token,
  action,
  message = "hello",
}: ReflectionTestHarnessProps) {
  const [status, setStatus] = useState<
    "connecting" | "waiting" | "done" | "error"
  >("connecting");
  const [errorMsg, setErrorMsg] = useState<string>("");

  useEffect(() => {
    let room: Room | null = null;

    const run = async () => {
      try {
        room = new Room();
        await room.connect(url, token);
        await waitForConnected(room);
        await waitForParticipant(room, SERVER_IDENTITY);
        await waitForDataChannelReady(room);
        setStatus("waiting");

        const transport = createLiveKitTransport({
          room,
          targetIdentity: SERVER_IDENTITY,
          debug: true,
        });
        const reflectionClient = createClient(ServerReflection, transport);

        // 1. list_services — always exercised first.
        const listReq = create(ServerReflectionRequestSchema, {
          messageRequest: { case: "listServices", value: "*" },
        });
        const serviceNames: string[] = [];
        for await (const resp of reflectionClient.serverReflectionInfo(
          once(listReq),
        )) {
          if (resp.messageResponse.case === "listServicesResponse") {
            for (const s of resp.messageResponse.value.service) {
              serviceNames.push(s.name);
            }
          }
        }
        // NOTE: test checks for "list_services:" (underscore) — keep the key consistent.
        logReflection(`list_services: ${JSON.stringify(serviceNames)}`);

        if (action === "list-services") {
          setStatus("done");
          return;
        }

        // 2. Fetch the EchoService descriptor and build a runtime registry.
        const symReq = create(ServerReflectionRequestSchema, {
          messageRequest: {
            case: "fileContainingSymbol",
            value: "test.EchoService",
          },
        });
        const fileProtos: Uint8Array[] = [];
        for await (const resp of reflectionClient.serverReflectionInfo(
          once(symReq),
        )) {
          if (resp.messageResponse.case === "fileDescriptorResponse") {
            for (const b of resp.messageResponse.value.fileDescriptorProto) {
              fileProtos.push(b);
            }
          }
        }
        const fds = fromBinary(
          FileDescriptorSetSchema,
          encodeFileDescriptorSet(fileProtos),
        );
        const registry = createFileRegistry(fds);
        const svc = registry.getService("test.EchoService");
        if (!svc) throw new Error("test.EchoService not found in registry");

        if (action === "dynamic-invoke-echo") {
          const method = svc.methods.find((m) => m.name === "Echo");
          if (!method) throw new Error("Echo method not found");
          logReflection(`resolved method: ${method.name} (${method.methodKind})`);

          if (method.input.typeName !== EchoRequestSchema.typeName) {
            throw new Error(`unexpected input type: ${method.input.typeName}`);
          }
          if (method.output.typeName !== EchoResponseSchema.typeName) {
            throw new Error(`unexpected output type: ${method.output.typeName}`);
          }

          const response = await transport.unary(
            method as never,
            undefined,
            undefined,
            undefined,
            { message } as never,
          );
          logReflection(
            `invoke result: ${JSON.stringify(response.message)}`,
          );
          setStatus("done");
          return;
        }

        if (action === "dynamic-invoke-server-stream") {
          const method = svc.methods.find((m) => m.name === "EchoServerStream");
          if (!method) throw new Error("EchoServerStream method not found");
          const words = message.split(" ");
          const response = await transport.stream(
            method as never,
            undefined,
            undefined,
            undefined,
            (async function* () { yield { message: words[0] ?? message } as never; })(),
          );
          let chunkIdx = 0;
          for await (const msg of response.message) {
            chunkIdx += 1;
            logReflection(
              `stream chunk: ${toJsonString(method.output as never, msg as never)}`,
            );
          }
          logReflection(`stream done: ${chunkIdx} chunks`);
          setStatus("done");
          return;
        }

        if (action === "dynamic-invoke-bidi-stream") {
          const method = svc.methods.find((m) => m.name === "EchoBidiStream");
          if (!method) throw new Error("EchoBidiStream method not found");
          const words = message.split(" ");
          const response = await transport.stream(
            method as never,
            undefined,
            undefined,
            undefined,
            (async function* () {
              for (const w of words) {
                yield { message: w } as never;
              }
            })(),
          );
          for await (const msg of response.message) {
            logReflection(
              `stream chunk: ${toJsonString(method.output as never, msg as never)}`,
            );
          }
          setStatus("done");
          return;
        }

        throw new Error(`Unknown action: ${action}`);
      } catch (e) {
        const err = e instanceof Error ? e : new Error(String(e));
        logReflection(`error: ${err.message}`);
        setErrorMsg(err.message);
        setStatus("error");
        (window as unknown as { cypressLastError?: unknown }).cypressLastError =
          e;
      } finally {
        if (room) room.disconnect();
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
