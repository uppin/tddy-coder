/**
 * Wraps any ConnectRPC {@link Transport} with the same session-token refresh gate used on HTTP.
 *
 * LiveKit daemon RPC (`useDaemonClient`) must refresh `sessionToken` per unary call — otherwise a
 * terminal keystroke after the 5-minute access-token TTL sends a stale token and the daemon returns
 * `invalid or expired session` while the LiveKit room itself stays connected.
 */

import type { DescMessage, MessageInitShape } from "@bufbuild/protobuf";
import type {
  DescMethodStreaming,
  DescMethodUnary,
  StreamResponse,
  Transport,
  UnaryResponse,
} from "@connectrpc/connect";
import { carriesSessionToken } from "./authGateInterceptor";

async function refreshSessionTokenOnMessage(
  message: MessageInitShape<DescMessage>,
  ensureFreshAccessToken: () => Promise<string | null>,
): Promise<void> {
  if (!carriesSessionToken(message)) return;
  const token = await ensureFreshAccessToken();
  if (token !== null) {
    message.sessionToken = token;
  }
}

/**
 * Return a transport that rewrites `sessionToken` on every unary request and on the first message
 * of each client/server/bidi stream before delegating to `inner`.
 */
export function wrapTransportWithAuthGate(
  inner: Transport,
  ensureFreshAccessToken: () => Promise<string | null>,
): Transport {
  return {
    unary<I extends DescMessage, O extends DescMessage>(
      method: DescMethodUnary<I, O>,
      signal: AbortSignal | undefined,
      timeoutMs: number | undefined,
      header: HeadersInit | undefined,
      input: MessageInitShape<I>,
    ): Promise<UnaryResponse<I, O>> {
      return refreshSessionTokenOnMessage(input, ensureFreshAccessToken).then(() =>
        inner.unary(method, signal, timeoutMs, header, input),
      );
    },

    stream<I extends DescMessage, O extends DescMessage>(
      method: DescMethodStreaming<I, O>,
      signal: AbortSignal | undefined,
      timeoutMs: number | undefined,
      header: HeadersInit | undefined,
      input: AsyncIterable<MessageInitShape<I>>,
    ): Promise<StreamResponse<I, O>> {
      async function* gatedInput(): AsyncGenerator<MessageInitShape<I>> {
        let first = true;
        for await (const item of input) {
          if (first) {
            await refreshSessionTokenOnMessage(item, ensureFreshAccessToken);
            first = false;
          }
          yield item;
        }
      }
      return inner.stream(method, signal, timeoutMs, header, gatedInput());
    },
  };
}
