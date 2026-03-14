/**
 * ConnectRPC integration tests for EchoService.Echo (unary).
 *
 * Prerequisite: Start tddy-coder with web server:
 *   cargo run -p tddy-coder -- --daemon --web-port 8888 --web-bundle-path packages/tddy-web/dist
 *
 * Set CONNECTRPC_BASE_URL=http://127.0.0.1:PORT/rpc to override.
 * Set CONNECTRPC_TEST_SKIP=1 to skip (e.g. when server not available in CI).
 */

import { createConnectTransport } from "@connectrpc/connect-node";
import { createClient } from "@connectrpc/connect";
import { create } from "@bufbuild/protobuf";
import { EchoService, EchoRequestSchema } from "../gen/test/echo_service_pb";

const baseUrl = process.env.CONNECTRPC_BASE_URL ?? "http://127.0.0.1:8888/rpc";

const transport = createConnectTransport({
  baseUrl,
  httpVersion: "1.1",
  useBinaryFormat: true,
});

const client = createClient(EchoService, transport);

describe("EchoService.Echo (unary)", () => {
  test.skipIf(process.env.CONNECTRPC_TEST_SKIP === "1")("echoes message", async () => {
    const res = await client.echo(create(EchoRequestSchema, { message: "hello" }));
    expect(res.message).toBe("hello");
    expect(typeof res.timestamp).toBe("bigint");
  });
});
