/**
 * tddy-connectrpc-testkit
 *
 * In-memory ConnectRPC backend for bun:test and Cypress component tests.
 * Framework-agnostic: produces a plain `Transport` usable with `createClient`.
 *
 * @example
 * ```ts
 * import { anInMemoryRpcBackend } from "tddy-connectrpc-testkit";
 * import { createClient } from "@connectrpc/connect";
 * import { TokenService } from "../gen/token_pb";
 *
 * const backend = anInMemoryRpcBackend()
 *   .onUnary(TokenService.method.generateToken, (req) => ({
 *     token: "fake-token",
 *     ttlSeconds: 3600n,
 *   }));
 *
 * const client = createClient(TokenService, backend.transport());
 * const res = await client.generateToken({ room: "test", identity: "alice" });
 * // res.token === "fake-token"
 *
 * const calls = backend.callsTo(TokenService.method.generateToken);
 * // calls[0].room === "test"
 * ```
 */

export { anInMemoryRpcBackend, InMemoryRpcBackend } from "./backend.js";
export type { RecordedUnaryCall } from "./backend.js";
