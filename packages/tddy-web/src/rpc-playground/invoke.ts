/**
 * Invoke an arbitrary RPC method (resolved dynamically from a reflection registry)
 * over a ConnectRPC transport, taking a JSON request and returning a JSON result.
 */
import { Code, ConnectError, type Transport } from "@connectrpc/connect";
import {
  fromJson,
  toJsonString,
  type DescMethod,
  type DescMethodStreaming,
  type DescMethodUnary,
} from "@bufbuild/protobuf";

export type InvokeResult =
  | { kind: "success"; json: string }
  | { kind: "error"; code: string; message: string }
  | { kind: "stream_complete"; chunks: string[] };

async function* singleInput(message: unknown): AsyncIterable<unknown> {
  yield message;
}

export async function invokeRpc(
  transport: Transport,
  method: DescMethod,
  requestJson: string,
): Promise<InvokeResult> {
  try {
    const parsed = JSON.parse(requestJson.trim() === "" ? "{}" : requestJson);

    if (method.methodKind === "unary") {
      const unaryMethod = method as DescMethodUnary;
      const input = fromJson(unaryMethod.input, parsed);
      const response = await transport.unary(
        unaryMethod,
        undefined,
        undefined,
        undefined,
        input,
      );
      return {
        kind: "success",
        json: toJsonString(unaryMethod.output, response.message, {
          prettySpaces: 2,
        }),
      };
    }

    // server_streaming, client_streaming, bidi_streaming
    const streamMethod = method as DescMethodStreaming;
    const input = fromJson(streamMethod.input, parsed);
    const response = await transport.stream(
      streamMethod,
      undefined,
      undefined,
      undefined,
      singleInput(input) as AsyncIterable<never>,
    );
    const chunks: string[] = [];
    for await (const message of response.message) {
      chunks.push(
        toJsonString(streamMethod.output, message, { prettySpaces: 2 }),
      );
    }
    return { kind: "stream_complete", chunks };
  } catch (e) {
    if (e instanceof ConnectError) {
      const name = Code[e.code] as string | undefined;
      const code = name
        ? name[0].toLowerCase() +
          name.substring(1).replace(/[A-Z]/g, (c) => "_" + c.toLowerCase())
        : e.code.toString();
      return { kind: "error", code, message: e.message };
    }
    return { kind: "error", code: "unknown", message: String(e) };
  }
}
