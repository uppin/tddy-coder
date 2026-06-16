/**
 * RPC Playground — dynamic invocation tests.
 *
 * Tests `invokeRpc` in invoke.ts against a fake @connectrpc/connect Transport.
 * Verifies that:
 *   - unary requests are serialized and responses decoded correctly
 *   - the transport receives the correct service typeName and method name
 *   - ConnectError is mapped to { code, message }
 *   - server-streaming collects multiple decoded JSON chunks
 *
 * All imports fail until invoke.ts is created (red phase).
 */

import { describe, expect, it, mock } from "bun:test";
import type { Transport, UnaryRequest, UnaryResponse, StreamResponse } from "@connectrpc/connect";
import type { DescMethod } from "@bufbuild/protobuf";
import { ConnectError, Code } from "@connectrpc/connect";

// These imports fail until invoke.ts is created.
import { invokeRpc, type InvokeResult } from "./invoke";

// Import needed for building a fake DescMethod from the real echo service descriptor.
// Fails until reflection / registry machinery is in place.
import { buildRegistry, findMethod } from "./registry";
import { ECHO_SERVICE_DESCRIPTOR_FIXTURE } from "./test-fixtures/echoServiceDescriptor";

// ---------------------------------------------------------------------------
// Fake transport helpers
// ---------------------------------------------------------------------------

function makeFakeUnaryTransport(responseJson: string): Transport {
  return {
    async unary(method: DescMethod, _signal, _timeout, _header, message): Promise<UnaryResponse> {
      // Echo back what was sent, decoded through the method output schema.
      // In the real transport, message is deserialized; here we return a preset JSON.
      const { create, fromJsonString } = await import("@bufbuild/protobuf");
      const output = fromJsonString(method.output, responseJson);
      return {
        stream: false,
        service: method.parent,
        method,
        header: new Headers(),
        trailer: new Headers(),
        message: output,
      } as unknown as UnaryResponse;
    },
    stream() {
      throw new Error("stream not expected in this test");
    },
  } as unknown as Transport;
}

function makeFakeErrorTransport(code: Code): Transport {
  return {
    async unary() {
      throw new ConnectError("test error", code);
    },
    stream() {
      throw new ConnectError("test error", code);
    },
  } as unknown as Transport;
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

describe("invokeRpc — unary", () => {
  it("serializes request JSON and returns decoded JSON response", async () => {
    const registry = buildRegistry(ECHO_SERVICE_DESCRIPTOR_FIXTURE);
    const method = findMethod(registry, "test.EchoService", "Echo");
    const transport = makeFakeUnaryTransport('{"message":"hi","timestamp":"0"}');

    const result = await invokeRpc(transport, method, '{"message":"hi"}');

    expect(result.kind).toBe("success");
    if (result.kind === "success") {
      const parsed = JSON.parse(result.json);
      expect(parsed.message).toBe("hi");
    }
  });

  it("passes correct service typeName and method name to the transport", async () => {
    const registry = buildRegistry(ECHO_SERVICE_DESCRIPTOR_FIXTURE);
    const method = findMethod(registry, "test.EchoService", "Echo");

    let capturedMethod: DescMethod | undefined;
    const capturingTransport: Transport = {
      async unary(m) {
        capturedMethod = m;
        const { fromJsonString } = await import("@bufbuild/protobuf");
        const output = fromJsonString(m.output, '{"message":"","timestamp":"0"}');
        return {
          stream: false,
          service: m.parent,
          method: m,
          header: new Headers(),
          trailer: new Headers(),
          message: output,
        } as unknown as UnaryResponse;
      },
      stream() { throw new Error("not expected"); },
    } as unknown as Transport;

    await invokeRpc(capturingTransport, method, "{}");

    expect(capturedMethod).toBeDefined();
    expect(capturedMethod!.parent.typeName).toBe("test.EchoService");
    expect(capturedMethod!.name).toBe("Echo");
  });

  it("maps ConnectError NOT_FOUND to { kind: 'error', code, message }", async () => {
    const registry = buildRegistry(ECHO_SERVICE_DESCRIPTOR_FIXTURE);
    const method = findMethod(registry, "test.EchoService", "Echo");
    const transport = makeFakeErrorTransport(Code.NotFound);

    const result = await invokeRpc(transport, method, "{}");

    expect(result.kind).toBe("error");
    if (result.kind === "error") {
      expect(result.code).toBe("not_found");
      expect(typeof result.message).toBe("string");
    }
  });

  it("maps ConnectError INVALID_ARGUMENT to { kind: 'error', code, message }", async () => {
    const registry = buildRegistry(ECHO_SERVICE_DESCRIPTOR_FIXTURE);
    const method = findMethod(registry, "test.EchoService", "Echo");
    const transport = makeFakeErrorTransport(Code.InvalidArgument);

    const result = await invokeRpc(transport, method, "{}");

    expect(result.kind).toBe("error");
    if (result.kind === "error") {
      expect(result.code).toBe("invalid_argument");
    }
  });
});

describe("invokeRpc — server streaming", () => {
  it("collects multiple decoded JSON chunks from a server-streaming method", async () => {
    const registry = buildRegistry(ECHO_SERVICE_DESCRIPTOR_FIXTURE);
    const method = findMethod(registry, "test.EchoService", "EchoServerStream");

    // Fake transport yields 3 response frames.
    const fakeChunks = [
      '{"message":"streaming #1","timestamp":"0"}',
      '{"message":"streaming #2","timestamp":"0"}',
      '{"message":"streaming #3","timestamp":"0"}',
    ];

    // NOTE: transport.stream() must return Promise<StreamResponse> (not an AsyncGenerator).
    // Each item yielded by StreamResponse.message is the proto message directly.
    const streamingTransport: Transport = {
      unary() { throw new Error("not expected"); },
      async stream(m) {
        const { fromJsonString } = await import("@bufbuild/protobuf");
        return {
          stream: true,
          method: m,
          header: new Headers(),
          trailer: new Headers(),
          message: (async function* () {
            for (const chunk of fakeChunks) {
              yield fromJsonString(m.output, chunk);
            }
          })(),
        };
      },
    } as unknown as Transport;

    const result = await invokeRpc(streamingTransport, method, '{"message":"streaming"}');

    expect(result.kind).toBe("stream_complete");
    if (result.kind === "stream_complete") {
      expect(result.chunks).toHaveLength(3);
      const first = JSON.parse(result.chunks[0]);
      expect(first.message).toBe("streaming #1");
      const last = JSON.parse(result.chunks[2]);
      expect(last.message).toBe("streaming #3");
    }
  });
});
