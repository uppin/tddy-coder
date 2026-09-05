/**
 * Apply a ConnectRPC interceptor stack to any {@link Transport}.
 *
 * `createConnectTransport` takes `interceptors` because it builds the `UnaryRequest` /
 * `StreamRequest` an interceptor sees on its way to the HTTP client. A transport that is not an
 * HTTP one — the webview-IPC flavour, for instance — builds no such request, so an interceptor has
 * nothing to wrap.
 *
 * Rather than reimplement traffic metering and the auth gate a second time against a different
 * seam, this wrapper builds the same request shape and runs the *same* `Interceptor` functions
 * around a call, so both flavours carry one stack and cannot drift apart. `@connectrpc/connect`
 * keeps its own `applyInterceptors` internal, so the (four-line) composition is reproduced here.
 */

import { create } from "@bufbuild/protobuf";
import type {
  DescMessage,
  DescMethodStreaming,
  DescMethodUnary,
  MessageInitShape,
  MessageShape,
} from "@bufbuild/protobuf";
import {
  createContextValues,
  type ContextValues,
  type Interceptor,
  type StreamRequest,
  type StreamResponse,
  type Transport,
  type UnaryRequest,
  type UnaryResponse,
} from "@connectrpc/connect";

/** What an interceptor wraps: one call invocation. Mirrors the library's own (unexported) `AnyFn`. */
type CallFn = (
  req: UnaryRequest | StreamRequest,
) => Promise<UnaryResponse | StreamResponse>;

export interface InterceptedTransportOptions {
  /** The transport that actually performs the call. */
  inner: Transport;
  /** Outermost first, matching `createConnectTransport`'s `interceptors` option. */
  interceptors: readonly Interceptor[];
  /**
   * Prefix for the `url` an interceptor reads — this transport reaches no URL, so the value names
   * the pipe the call travels down (`webview-ipc://daemon`) instead of pretending to be an origin.
   */
  baseUrl: string;
}

/** `{baseUrl}/{package.Service}/{Method}`, the URL shape the Connect protocol gives a call. */
function methodUrl(baseUrl: string, method: DescMethodUnary | DescMethodStreaming): string {
  return `${baseUrl.replace(/\/+$/, "")}/${method.parent.typeName}/${method.name}`;
}

/** Compose the stack so `interceptors[0]` is the outermost layer, as the library documents. */
function applyInterceptors(last: CallFn, interceptors: readonly Interceptor[]): CallFn {
  return interceptors.reduceRight<CallFn>((next, interceptor) => interceptor(next), last);
}

/** `options.inner`, with `options.interceptors` layered around every unary and streaming call. */
export function transportWithInterceptors({
  inner,
  interceptors,
  baseUrl,
}: InterceptedTransportOptions): Transport {
  return {
    async unary<I extends DescMessage, O extends DescMessage>(
      method: DescMethodUnary<I, O>,
      signal: AbortSignal | undefined,
      timeoutMs: number | undefined,
      header: HeadersInit | undefined,
      input: MessageInitShape<I>,
      contextValues?: ContextValues,
    ): Promise<UnaryResponse<I, O>> {
      const request: UnaryRequest<I, O> = {
        stream: false,
        service: method.parent,
        method,
        requestMethod: "POST",
        url: methodUrl(baseUrl, method),
        // A call with no caller-supplied signal is one that is never aborted; interceptors read
        // `signal` unconditionally, so it is a real signal rather than `undefined`.
        signal: signal ?? new AbortController().signal,
        header: new Headers(header),
        contextValues: contextValues ?? createContextValues(),
        message: create(method.input, input),
      };
      const call = applyInterceptors(
        (req) =>
          inner.unary(
            req.method as DescMethodUnary<I, O>,
            req.signal,
            timeoutMs,
            req.header,
            req.message as MessageInitShape<I>,
            req.contextValues,
          ),
        interceptors,
      );
      return (await call(request)) as UnaryResponse<I, O>;
    },

    async stream<I extends DescMessage, O extends DescMessage>(
      method: DescMethodStreaming<I, O>,
      signal: AbortSignal | undefined,
      timeoutMs: number | undefined,
      header: HeadersInit | undefined,
      input: AsyncIterable<MessageInitShape<I>>,
      contextValues?: ContextValues,
    ): Promise<StreamResponse<I, O>> {
      async function* messages(): AsyncGenerator<MessageShape<I>> {
        for await (const item of input) {
          yield create(method.input, item);
        }
      }
      const request: StreamRequest<I, O> = {
        stream: true,
        service: method.parent,
        method,
        requestMethod: "POST",
        url: methodUrl(baseUrl, method),
        signal: signal ?? new AbortController().signal,
        header: new Headers(header),
        contextValues: contextValues ?? createContextValues(),
        message: messages(),
      };
      const call = applyInterceptors(
        (req) =>
          inner.stream(
            req.method as DescMethodStreaming<I, O>,
            req.signal,
            timeoutMs,
            req.header,
            req.message as AsyncIterable<MessageInitShape<I>>,
            req.contextValues,
          ),
        interceptors,
      );
      return (await call(request)) as StreamResponse<I, O>;
    },
  };
}
