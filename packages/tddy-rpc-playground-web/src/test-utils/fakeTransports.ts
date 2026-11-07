/**
 * Fake Connect transports for the playground's `bun test` suites — moved with the package from
 * `tddy-web`'s `src/test-utils` (their only consumer is `src/invoke.test.ts`).
 */
import type { DescMethod } from "@bufbuild/protobuf";
import { ConnectError, type Code, type Transport, type UnaryResponse } from "@connectrpc/connect";

/** Fake unary Transport that returns a preset JSON response string decoded through the method's output schema. */
export function aFakeUnaryTransport(responseJson: string): Transport {
  return {
    async unary(method: DescMethod): Promise<UnaryResponse> {
      const { fromJsonString } = await import("@bufbuild/protobuf");
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
      throw new Error("stream not expected in this transport");
    },
  } as unknown as Transport;
}

/** Fake Transport that rejects every call with a ConnectError of the given code. */
export function aFakeErrorTransport(code: Code): Transport {
  return {
    async unary() {
      throw new ConnectError("test error", code);
    },
    stream() {
      throw new ConnectError("test error", code);
    },
  } as unknown as Transport;
}

/** Fake server-streaming Transport that yields each string in `chunks` decoded through the method's output schema. */
export function aFakeStreamingTransport(chunks: string[]): Transport {
  return {
    unary() {
      throw new Error("unary not expected in this transport");
    },
    async stream(m: DescMethod) {
      const { fromJsonString } = await import("@bufbuild/protobuf");
      return {
        stream: true,
        method: m,
        header: new Headers(),
        trailer: new Headers(),
        message: (async function* () {
          for (const chunk of chunks) {
            yield fromJsonString(m.output, chunk);
          }
        })(),
      };
    },
  } as unknown as Transport;
}
