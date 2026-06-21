/**
 * RPC Playground — descriptor registry tests.
 *
 * Tests `buildRegistry` and `findMethod` in registry.ts. These functions parse a binary
 * FileDescriptorSet (obtained from the server via gRPC ServerReflection) into a
 * `@bufbuild/protobuf` Registry, then look up DescMethod objects that LiveKitTransport can
 * invoke directly without a compiled ConnectES client.
 *
 * All imports will fail to resolve until registry.ts is created (red phase).
 */

import { beforeEach, describe, expect, it } from "bun:test";
// These imports fail until registry.ts is created.
import { buildRegistry, findMethod } from "./registry";

// Fixture: a binary FileDescriptorSet covering test.EchoService.
// In green phase this will be loaded from a checked-in .bin fixture.
// For now the import itself will fail.
import { ECHO_SERVICE_DESCRIPTOR_FIXTURE } from "./test-fixtures/echoServiceDescriptor";

describe("buildRegistry + findMethod", () => {
  let registry: ReturnType<typeof buildRegistry>;

  beforeEach(() => {
    // Given: a registry built from the EchoService FileDescriptorSet fixture
    registry = buildRegistry(ECHO_SERVICE_DESCRIPTOR_FIXTURE);
  });

  // -------------------------------------------------------------------------
  // Echo — unary method
  // -------------------------------------------------------------------------

  it("Echo method belongs to the test.EchoService service type", () => {
    // When
    const method = findMethod(registry, "test.EchoService", "Echo");

    // Then
    expect(method.parent.typeName).toBe("test.EchoService");
  });

  it("Echo method name and kind resolve to unary", () => {
    // When
    const method = findMethod(registry, "test.EchoService", "Echo");

    // Then
    expect(method.name).toBe("Echo");
    expect(method.methodKind).toBe("unary");
  });

  it("Echo method takes EchoRequest input and produces EchoResponse output", () => {
    // When
    const method = findMethod(registry, "test.EchoService", "Echo");

    // Then
    expect(method.input.typeName).toBe("test.EchoRequest");
    expect(method.output.typeName).toBe("test.EchoResponse");
  });

  // -------------------------------------------------------------------------
  // Streaming method kinds
  // -------------------------------------------------------------------------

  it("EchoServerStream has server_streaming kind", () => {
    // When
    const method = findMethod(registry, "test.EchoService", "EchoServerStream");

    // Then
    expect(method.methodKind).toBe("server_streaming");
  });

  it("EchoClientStream has client_streaming kind", () => {
    // When
    const method = findMethod(registry, "test.EchoService", "EchoClientStream");

    // Then
    expect(method.methodKind).toBe("client_streaming");
  });

  it("EchoBidiStream has bidi_streaming kind", () => {
    // When
    const method = findMethod(registry, "test.EchoService", "EchoBidiStream");

    // Then
    expect(method.methodKind).toBe("bidi_streaming");
  });

  // -------------------------------------------------------------------------
  // Error handling
  // -------------------------------------------------------------------------

  it("throws when looking up a service that does not exist in the descriptor", () => {
    // When / Then
    expect(() => findMethod(registry, "nonexistent.FakeService", "Foo")).toThrow();
  });

  it("throws when looking up a method that does not exist on a known service", () => {
    // When / Then
    expect(() => findMethod(registry, "test.EchoService", "NonExistentMethod")).toThrow();
  });
});
