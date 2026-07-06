/**
 * Unit tests for wrapTransportWithAuthGate — the transport wrapper that refreshes `sessionToken`
 * on every unary RPC and on the first message of each stream before delegating to the inner
 * transport. LiveKit daemon RPC uses this wrapper so terminal input after the access-token TTL
 * still reaches the daemon with a fresh token.
 */

import { describe, it, expect } from "bun:test";
import { create } from "@bufbuild/protobuf";
import type { Transport } from "@connectrpc/connect";
import {
  ConnectionService,
  ListSessionsRequestSchema,
  SessionTerminalInputSchema,
  StreamTerminalOutputRequestSchema,
} from "../gen/connection_pb";
import { wrapTransportWithAuthGate } from "./authGatedTransport";

function aRecordingTransport(): Transport & {
  unaryInputs: unknown[];
  streamFirstInputs: unknown[];
} {
  const unaryInputs: unknown[] = [];
  const streamFirstInputs: unknown[] = [];
  return {
    unaryInputs,
    streamFirstInputs,
    async unary(_method, _signal, _timeoutMs, _header, input) {
      unaryInputs.push(input);
      return {
        stream: false as const,
        service: ConnectionService,
        method: ConnectionService.method.listSessions,
        message: {},
        header: new Headers(),
        trailer: new Headers(),
      };
    },
    async stream(_method, _signal, _timeoutMs, _header, input) {
      for await (const item of input) {
        streamFirstInputs.push(item);
        break;
      }
      return {
        stream: true as const,
        service: ConnectionService,
        method: ConnectionService.method.streamTerminalOutput,
        message: (async function* () {})(),
        header: new Headers(),
        trailer: new Headers(),
      };
    },
  };
}

describe("wrapTransportWithAuthGate", () => {
  it("rewrites sessionToken on unary RPC before delegating to the inner transport", async () => {
    // Given — an inner transport that records inputs and a gate that yields a fresh token
    const inner = aRecordingTransport();
    const gated = wrapTransportWithAuthGate(inner, async () => "fresh-access-token");

    // When — SendTerminalInput is sent with a stale token
    await gated.unary(
      ConnectionService.method.sendTerminalInput,
      undefined,
      undefined,
      undefined,
      create(SessionTerminalInputSchema, {
        sessionToken: "stale-token",
        sessionId: "sess-1",
        data: new Uint8Array([13]),
      }),
    );

    // Then — the inner transport received the refreshed token
    expect((inner.unaryInputs[0] as { sessionToken: string }).sessionToken).toBe("fresh-access-token");
  });

  it("refreshes sessionToken on the first message of a client stream", async () => {
    // Given — a gated transport and a stale token on the stream open request
    const inner = aRecordingTransport();
    const gated = wrapTransportWithAuthGate(inner, async () => "fresh-access-token");

    async function* input() {
      yield create(StreamTerminalOutputRequestSchema, {
        sessionToken: "stale-token",
        sessionId: "sess-1",
      });
    }

    // When — the terminal output stream is opened
    await gated.stream(
      ConnectionService.method.streamTerminalOutput,
      undefined,
      undefined,
      undefined,
      input(),
    );

    // Then — the first stream message carries the fresh token
    expect((inner.streamFirstInputs[0] as { sessionToken: string }).sessionToken).toBe(
      "fresh-access-token",
    );
  });

  it("waits for an in-flight refresh before forwarding a unary RPC", async () => {
    // Given — a refresh that has not yet completed
    let resolveToken: (t: string) => void = () => {};
    const pending = new Promise<string>((resolve) => {
      resolveToken = resolve;
    });
    const inner = aRecordingTransport();
    const gated = wrapTransportWithAuthGate(inner, () => pending);
    const call = gated.unary(
      ConnectionService.method.listSessions,
      undefined,
      undefined,
      undefined,
      create(ListSessionsRequestSchema, { sessionToken: "stale-token" }),
    );
    await Promise.resolve();

    // Then — the inner transport has not been called yet
    expect(inner.unaryInputs).toHaveLength(0);

    // When — the refresh completes
    resolveToken("fresh-access-token");
    await call;

    // Then — the request was forwarded with the fresh token
    expect((inner.unaryInputs[0] as { sessionToken: string }).sessionToken).toBe("fresh-access-token");
  });
});
