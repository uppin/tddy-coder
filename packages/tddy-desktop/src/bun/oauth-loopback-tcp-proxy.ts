/**
 * Raw TCP listener on the operator machine; each accepted socket is piped through
 * `LoopbackTunnelService.StreamBytes` to `127.0.0.1:remoteLoopbackPort` on the session host.
 */
import { create } from "@bufbuild/protobuf";
import { createClient } from "@connectrpc/connect";
import type { Room as BrowserRoom } from "livekit-client";
import type { Socket } from "bun";
import {
  createLiveKitTransport,
  LoopbackTunnelService,
  TunnelChunkSchema,
} from "tddy-livekit-web";
import type { Room } from "@livekit/rtc-node";

type ConnState = {
  queue: Uint8Array[];
  closed: boolean;
};

const connState = new WeakMap<Socket, ConnState>();

function toUint8Array(data: Buffer | Uint8Array): Uint8Array {
  if (data instanceof Uint8Array && !(data instanceof Buffer)) {
    return data;
  }
  return new Uint8Array(data.buffer, data.byteOffset, data.byteLength);
}

async function pipeSocketThroughTunnel(
  socket: Socket,
  room: Room,
  targetIdentity: string,
  remoteLoopbackPort: number,
): Promise<void> {
  const transport = createLiveKitTransport({
    room: room as unknown as BrowserRoom,
    targetIdentity,
  });
  const client = createClient(LoopbackTunnelService, transport);
  const state = connState.get(socket);
  if (!state) {
    transport.destroy();
    return;
  }

  try {
    async function* inputGen() {
      yield create(TunnelChunkSchema, {
        openPort: remoteLoopbackPort,
        data: new Uint8Array(0),
      });
      while (true) {
        if (state.queue.length > 0) {
          const chunks = state.queue.splice(0);
          const total = chunks.reduce((sum, c) => sum + c.length, 0);
          const data = new Uint8Array(total);
          let offset = 0;
          for (const c of chunks) {
            data.set(c, offset);
            offset += c.length;
          }
          yield create(TunnelChunkSchema, { openPort: 0, data });
        } else if (state.closed) {
          break;
        } else {
          await new Promise((r) => setTimeout(r, 20));
        }
      }
    }

    const stream = client.streamBytes(inputGen());
    for await (const chunk of stream) {
      if (chunk.data.length > 0) {
        socket.write(chunk.data);
      }
    }
  } catch (e) {
    console.error("[tddy-desktop] OAuth loopback tunnel stream error:", e);
  } finally {
    transport.destroy();
    connState.delete(socket);
    socket.end();
  }
}

export type OAuthLoopbackTcpProxyHandle = {
  stop: () => void;
};

export function startOAuthLoopbackTcpProxy(options: {
  room: Room;
  targetIdentity: string;
  listenPort: number;
  /** Session-host loopback port (Codex listener); usually same as listenPort. */
  remoteLoopbackPort: number;
}): OAuthLoopbackTcpProxyHandle {
  const { room, targetIdentity, listenPort, remoteLoopbackPort } = options;

  const server = Bun.listen({
    hostname: "127.0.0.1",
    port: listenPort,
    socket: {
      open(socket) {
        connState.set(socket, { queue: [], closed: false });
        void pipeSocketThroughTunnel(
          socket,
          room,
          targetIdentity,
          remoteLoopbackPort,
        );
      },
      data(socket, data) {
        const state = connState.get(socket);
        if (!state || state.closed || data.byteLength === 0) {
          return;
        }
        state.queue.push(toUint8Array(data));
      },
      close(socket) {
        const state = connState.get(socket);
        if (state) {
          state.closed = true;
        }
      },
    },
  });

  return {
    stop: () => {
      server.stop();
    },
  };
}
