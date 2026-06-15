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

import { describe, expect, it } from "bun:test";
// These imports fail until registry.ts is created.
import { buildRegistry, findMethod } from "./registry";

// Fixture: a binary FileDescriptorSet covering test.EchoService.
// In green phase this will be loaded from a checked-in .bin fixture.
// For now the import itself will fail.
import { ECHO_SERVICE_DESCRIPTOR_FIXTURE } from "./test-fixtures/echoServiceDescriptor";

describe("buildRegistry", () => {
  it("resolves EchoService and Echo method from a FileDescriptorSet", () => {
    const registry = buildRegistry(ECHO_SERVICE_DESCRIPTOR_FIXTURE);
    const method = findMethod(registry, "test.EchoService", "Echo");

    expect(method.parent.typeName).toBe("test.EchoService");
    expect(method.name).toBe("Echo");
    expect(method.methodKind).toBe("unary");
    expect(method.input.typeName).toBe("test.EchoRequest");
    expect(method.output.typeName).toBe("test.EchoResponse");
  });

  it("returns server_streaming kind for EchoServerStream", () => {
    const registry = buildRegistry(ECHO_SERVICE_DESCRIPTOR_FIXTURE);
    const method = findMethod(registry, "test.EchoService", "EchoServerStream");
    expect(method.methodKind).toBe("server_streaming");
  });

  it("returns client_streaming kind for EchoClientStream", () => {
    const registry = buildRegistry(ECHO_SERVICE_DESCRIPTOR_FIXTURE);
    const method = findMethod(registry, "test.EchoService", "EchoClientStream");
    expect(method.methodKind).toBe("client_streaming");
  });

  it("returns bidi_streaming kind for EchoBidiStream", () => {
    const registry = buildRegistry(ECHO_SERVICE_DESCRIPTOR_FIXTURE);
    const method = findMethod(registry, "test.EchoService", "EchoBidiStream");
    expect(method.methodKind).toBe("bidi_streaming");
  });

  it("throws for an unknown service name", () => {
    const registry = buildRegistry(ECHO_SERVICE_DESCRIPTOR_FIXTURE);
    expect(() => findMethod(registry, "nonexistent.FakeService", "Foo")).toThrow();
  });

  it("throws for an unknown method on a known service", () => {
    const registry = buildRegistry(ECHO_SERVICE_DESCRIPTOR_FIXTURE);
    expect(() => findMethod(registry, "test.EchoService", "NonExistentMethod")).toThrow();
  });
});
