/**
 * An in-memory stand-in for the Tauri host application's IPC surface.
 *
 * The real host registers the page's response channel, receives `RpcRequest` frames through one
 * command, dispatches them against the daemon running in the same process, and writes `RpcResponse`
 * frames back down the channel. This double does exactly that, dispatching against an
 * `anInMemoryRpcBackend` transport — so a test drives the production webview-IPC transport end to
 * end, envelope frames and all, without a Tauri runtime.
 *
 * For bun:test only — never import from Cypress tests.
 */

import { create, fromBinary, toBinary } from "@bufbuild/protobuf";
import type { DescMethodUnary, DescService, Message, MessageInitShape } from "@bufbuild/protobuf";
import { Code, ConnectError, createClient, type Transport } from "@connectrpc/connect";
import { codeToString } from "@connectrpc/connect/protocol-connect";
import { RpcRequestSchema, RpcResponseSchema, type RpcRequest } from "tddy-rpc-web";
import type { WebviewIpcBridge } from "tddy-tauri-web";

export interface WebviewIpcHostDouble extends WebviewIpcBridge {
  /** The client epoch the page registered its response channel with. */
  connectedEpoch(): number;
}

/**
 * A host application serving `service` out of `transport` over its IPC bridge.
 *
 * Unary methods only: that is the whole surface the client-configuration and settings paths use,
 * and a half-implemented streaming leg would answer a streaming call with silence.
 */
export function aWebviewIpcHostServing(
  service: DescService,
  transport: Transport,
): WebviewIpcHostDouble {
  const client = createClient(service, transport) as unknown as Record<
    string,
    (input: unknown) => Promise<unknown>
  >;
  let onFrame: ((frame: Uint8Array) => void) | null = null;
  let epoch = 0;

  const write = (response: MessageInitShape<typeof RpcResponseSchema>) =>
    onFrame?.(toBinary(RpcResponseSchema, create(RpcResponseSchema, response)));

  const answer = (request: RpcRequest, responseMessage: Uint8Array) =>
    write({
      requestId: request.requestId,
      clientEpoch: request.clientEpoch,
      callMetadata: request.callMetadata,
      responseMessage,
      endOfStream: true,
    });

  const refuse = (request: RpcRequest, error: ConnectError) =>
    write({
      requestId: request.requestId,
      clientEpoch: request.clientEpoch,
      callMetadata: request.callMetadata,
      error: { code: codeToString(error.code), message: error.rawMessage },
      endOfStream: true,
    });

  const unaryMethodNamed = (name: string): DescMethodUnary | undefined =>
    service.methods.find(
      (candidate) => candidate.name === name && candidate.methodKind === "unary",
    ) as DescMethodUnary | undefined;

  return {
    async connect(handler, clientEpoch) {
      onFrame = handler;
      epoch = clientEpoch;
    },

    async send(frame) {
      const request = fromBinary(RpcRequestSchema, frame);
      const method = unaryMethodNamed(request.callMetadata?.method ?? "");
      if (!method) {
        refuse(
          request,
          new ConnectError(
            `${service.typeName} has no unary method ${request.callMetadata?.method ?? ""}`,
            Code.Unimplemented,
          ),
        );
        return;
      }
      await client[method.localName](fromBinary(method.input, request.requestMessage)).then(
        (response) => answer(request, toBinary(method.output, response as Message)),
        (error: unknown) => refuse(request, ConnectError.from(error)),
      );
    },

    // The real bridge resolves this when the host application is gone; this one never leaves.
    closed: new Promise<string>(() => {}),

    connectedEpoch() {
      return epoch;
    },
  };
}
