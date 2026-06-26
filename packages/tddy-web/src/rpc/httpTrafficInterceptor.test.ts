/**
 * Unit tests for createTrafficInterceptor — the ConnectRPC HTTP interceptor
 * that measures outbound (request) and inbound (response) bytes.
 *
 * Changeset: `byte-traffic`
 * PRD: `docs/ft/web/session-drawer.md` (Session Traffic Strip)
 */

import { describe, it, expect, beforeEach } from "bun:test";
import { createClient } from "@connectrpc/connect";
import { createConnectTransport } from "@connectrpc/connect-web";
import { create, toBinary } from "@bufbuild/protobuf";
import { anInMemoryRpcBackend } from "tddy-connectrpc-testkit";
import { TokenService, GenerateTokenRequestSchema, GenerateTokenResponseSchema } from "../gen/token_pb";

import { createTrafficInterceptor } from "./httpTrafficInterceptor";
import { TrafficMeter } from "./trafficMeter";

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

// Use create() so toBinary() can serialize them — protobuf-es v2 requires proto messages, not plain objects.
const FAKE_REQUEST = create(GenerateTokenRequestSchema, { room: "my-room", identity: "user-1", ttlSeconds: 3600n });
const FAKE_RESPONSE = create(GenerateTokenResponseSchema, { token: "eyJhbGciOiJIUzI1NiJ9.test.test", ttlSeconds: 3600n });

function buildTransportWithMeter(meter: TrafficMeter) {
  const backend = anInMemoryRpcBackend()
    .onUnary(TokenService.method.generateToken, () => FAKE_RESPONSE);
  const interceptor = createTrafficInterceptor(meter);
  // The in-memory backend transport accepts interceptors via wrapping.
  // We layer our interceptor on top of the router transport.
  return { backend, interceptor };
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

describe("createTrafficInterceptor", () => {
  let meter: TrafficMeter;

  beforeEach(() => {
    meter = new TrafficMeter();
  });

  it("records outbound bytes equal to the binary-serialized request message size", async () => {
    const { backend, interceptor } = buildTransportWithMeter(meter);
    const backendTransport = backend.transport();

    // We need to wrap the backend transport with our interceptor.
    // The createConnectTransport from connect-web accepts interceptors;
    // but it hits real HTTP. For unit testing we use createRouterTransport
    // from @connectrpc/connect (not connect-web) which also supports interceptors.
    // Since the testkit backend already uses createRouterTransport, we verify
    // interceptor behaviour by calling it manually with a fake request shape.

    // Given: expected outbound byte count from the known request
    const expectedOutBytes = toBinary(GenerateTokenRequestSchema, FAKE_REQUEST).length;

    // When: the interceptor is applied by calling it in the style of @connectrpc/connect
    const fakeReq = {
      stream: false,
      service: TokenService,
      method: TokenService.method.generateToken,
      message: FAKE_REQUEST,
      header: new Headers(),
      url: "/rpc/token.TokenService/GenerateToken",
      init: {},
      signal: new AbortController().signal,
    };
    const fakeRes = {
      stream: false,
      service: TokenService,
      method: TokenService.method.generateToken,
      message: FAKE_RESPONSE,
      header: new Headers(),
      trailer: new Headers(),
    };

    const next = async (_req: typeof fakeReq) => fakeRes;
    await interceptor(next as any)(fakeReq as any);

    // Then: meter recorded out bytes for the request
    expect(meter.snapshot().bytesOut).toBe(expectedOutBytes);
  });

  it("records inbound bytes equal to the binary-serialized response message size", async () => {
    const meter = new TrafficMeter();
    const interceptor = createTrafficInterceptor(meter);

    const expectedInBytes = toBinary(GenerateTokenResponseSchema, FAKE_RESPONSE).length;

    const fakeReq = {
      stream: false,
      service: TokenService,
      method: TokenService.method.generateToken,
      message: FAKE_REQUEST,
      header: new Headers(),
      url: "/rpc/token.TokenService/GenerateToken",
      init: {},
      signal: new AbortController().signal,
    };
    const fakeRes = {
      stream: false,
      service: TokenService,
      method: TokenService.method.generateToken,
      message: FAKE_RESPONSE,
      header: new Headers(),
      trailer: new Headers(),
    };

    const next = async (_req: typeof fakeReq) => fakeRes;
    await interceptor(next as any)(fakeReq as any);

    expect(meter.snapshot().bytesIn).toBe(expectedInBytes);
  });

  it("records outbound bytes before forwarding to next — inbound bytes are recorded after next() resolves", async () => {
    const interceptor = createTrafficInterceptor(meter);

    let bytesOutMidCall = -1;
    const fakeReq = {
      stream: false,
      service: TokenService,
      method: TokenService.method.generateToken,
      message: FAKE_REQUEST,
      header: new Headers(),
      url: "/rpc/token.TokenService/GenerateToken",
      init: {},
      signal: new AbortController().signal,
    };
    const fakeRes = {
      stream: false,
      service: TokenService,
      method: TokenService.method.generateToken,
      message: FAKE_RESPONSE,
      header: new Headers(),
      trailer: new Headers(),
    };

    const next = async (_req: typeof fakeReq) => {
      // During next(), out bytes should already be recorded (before the await)
      // but in bytes should not yet be recorded.
      bytesOutMidCall = meter.snapshot().bytesOut;
      return fakeRes;
    };
    await interceptor(next as any)(fakeReq as any);

    const expectedOutBytes = toBinary(GenerateTokenRequestSchema, FAKE_REQUEST).length;
    expect(bytesOutMidCall).toBe(expectedOutBytes);
    expect(meter.snapshot().bytesIn).toBeGreaterThan(0);
  });

  it("calls through to next — the RPC response is returned unchanged", async () => {
    const interceptor = createTrafficInterceptor(meter);

    const fakeReq = {
      stream: false,
      service: TokenService,
      method: TokenService.method.generateToken,
      message: FAKE_REQUEST,
      header: new Headers(),
      url: "/rpc/token.TokenService/GenerateToken",
      init: {},
      signal: new AbortController().signal,
    };
    const fakeRes = {
      stream: false,
      service: TokenService,
      method: TokenService.method.generateToken,
      message: FAKE_RESPONSE,
      header: new Headers(),
      trailer: new Headers(),
    };

    const next = async (_req: typeof fakeReq) => fakeRes;
    const result = await interceptor(next as any)(fakeReq as any);

    expect(result).toBe(fakeRes);
  });

  it("does not throw for streaming requests — passes through unchanged", async () => {
    const interceptor = createTrafficInterceptor(meter);

    const fakeStreamReq = {
      stream: true,
      service: TokenService,
      method: TokenService.method.generateToken,
      message: (async function* () {})(),
      header: new Headers(),
      url: "/rpc/token.TokenService/GenerateToken",
      init: {},
      signal: new AbortController().signal,
    };
    const fakeStreamRes = {
      stream: true,
      service: TokenService,
      method: TokenService.method.generateToken,
      message: (async function* () {})(),
      header: new Headers(),
      trailer: new Headers(),
    };

    const next = async (_req: typeof fakeStreamReq) => fakeStreamRes;
    const result = await interceptor(next as any)(fakeStreamReq as any);

    expect(result).toBeDefined();
    // Bytes should not have been counted yet (streaming is lazy)
    expect(meter.snapshot().bytesOut).toBe(0);
  });
});
