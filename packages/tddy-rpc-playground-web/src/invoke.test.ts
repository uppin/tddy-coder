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

import { beforeEach, describe, expect, it } from "bun:test";
import type { Transport, UnaryResponse } from "@connectrpc/connect";
import type { DescMethod } from "@bufbuild/protobuf";
import { Code } from "@connectrpc/connect";
import { aFakeErrorTransport, aFakeStreamingTransport, aFakeUnaryTransport } from "../test-utils";

// These imports fail until invoke.ts is created.
import { invokeRpc, type InvokeResult } from "./invoke";

// Import needed for building a fake DescMethod from the real echo service descriptor.
// Fails until reflection / registry machinery is in place.
import { buildRegistry, findMethod } from "./registry";
import { ECHO_SERVICE_DESCRIPTOR_FIXTURE } from "./test-fixtures/echoServiceDescriptor";

// ---------------------------------------------------------------------------
// Type-narrowing guards — eliminates conditional test bodies
// ---------------------------------------------------------------------------

function assertSuccess(
  result: InvokeResult,
): asserts result is Extract<InvokeResult, { kind: "success" }> {
  expect(result.kind).toBe("success");
  if (result.kind !== "success") throw new Error(`expected success, got: ${result.kind}`);
}

function assertError(
  result: InvokeResult,
): asserts result is Extract<InvokeResult, { kind: "error" }> {
  expect(result.kind).toBe("error");
  if (result.kind !== "error") throw new Error(`expected error, got: ${result.kind}`);
}

function assertStreamComplete(
  result: InvokeResult,
): asserts result is Extract<InvokeResult, { kind: "stream_complete" }> {
  expect(result.kind).toBe("stream_complete");
  if (result.kind !== "stream_complete") throw new Error(`expected stream_complete, got: ${result.kind}`);
}

// ---------------------------------------------------------------------------
// Tests — unary
// ---------------------------------------------------------------------------

describe("invokeRpc — unary", () => {
  let registry: ReturnType<typeof buildRegistry>;

  beforeEach(() => {
    // Given: a registry built from the EchoService FileDescriptorSet fixture
    registry = buildRegistry(ECHO_SERVICE_DESCRIPTOR_FIXTURE);
  });

  it("serializes request JSON and returns the decoded JSON response", async () => {
    // Given
    const method = findMethod(registry, "test.EchoService", "Echo");
    const transport = aFakeUnaryTransport('{"message":"hi","timestamp":"0"}');

    // When
    const result = await invokeRpc(transport, method, '{"message":"hi"}');

    // Then
    assertSuccess(result);
    const parsed = JSON.parse(result.json);
    expect(parsed.message).toBe("hi");
  });

  it("passes the correct service typeName and method name to the transport", async () => {
    // Given
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
      stream() {
        throw new Error("not expected");
      },
    } as unknown as Transport;

    // When
    await invokeRpc(capturingTransport, method, "{}");

    // Then
    expect(capturedMethod).toBeDefined();
    expect(capturedMethod!.parent.typeName).toBe("test.EchoService");
    expect(capturedMethod!.name).toBe("Echo");
  });

  it("maps ConnectError NOT_FOUND to { kind: 'error', code: 'not_found', message }", async () => {
    // Given
    const method = findMethod(registry, "test.EchoService", "Echo");
    const transport = aFakeErrorTransport(Code.NotFound);

    // When
    const result = await invokeRpc(transport, method, "{}");

    // Then
    assertError(result);
    expect(result.code).toBe("not_found");
    expect(typeof result.message).toBe("string");
  });

  it("maps ConnectError INVALID_ARGUMENT to { kind: 'error', code: 'invalid_argument' }", async () => {
    // Given
    const method = findMethod(registry, "test.EchoService", "Echo");
    const transport = aFakeErrorTransport(Code.InvalidArgument);

    // When
    const result = await invokeRpc(transport, method, "{}");

    // Then
    assertError(result);
    expect(result.code).toBe("invalid_argument");
  });
});

// ---------------------------------------------------------------------------
// Tests — server streaming
// ---------------------------------------------------------------------------

describe("invokeRpc — server streaming", () => {
  let registry: ReturnType<typeof buildRegistry>;

  beforeEach(() => {
    // Given: a registry built from the EchoService FileDescriptorSet fixture
    registry = buildRegistry(ECHO_SERVICE_DESCRIPTOR_FIXTURE);
  });

  it("collects all decoded JSON chunks from a server-streaming method", async () => {
    // Given
    const method = findMethod(registry, "test.EchoService", "EchoServerStream");
    const fakeChunks = [
      '{"message":"streaming #1","timestamp":"0"}',
      '{"message":"streaming #2","timestamp":"0"}',
      '{"message":"streaming #3","timestamp":"0"}',
    ];
    const transport = aFakeStreamingTransport(fakeChunks);

    // When
    const result = await invokeRpc(transport, method, '{"message":"streaming"}');

    // Then
    assertStreamComplete(result);
    expect(result.chunks).toHaveLength(3);
    expect(JSON.parse(result.chunks[0]).message).toBe("streaming #1");
    expect(JSON.parse(result.chunks[2]).message).toBe("streaming #3");
  });
});
