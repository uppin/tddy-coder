/**
 * In-memory ConnectRPC backend for bun:test and Cypress component tests.
 *
 * Wraps `createRouterTransport` from @connectrpc/connect with a fluent builder
 * API and call recording, so tests can register service implementations and
 * assert on the requests that were received.
 *
 * Framework-agnostic: the core produces a plain `Transport` that works with
 * `createClient(Service, transport)` in any runtime.
 *
 * @example
 * ```ts
 * const backend = anInMemoryRpcBackend()
 *   .implement(TokenService, {
 *     generateToken: async (req) => ({ token: "fake-token", ttlSeconds: 3600n }),
 *   })
 *   .onUnary(AuthService.method.getAuthStatus, () => ({ isAuthenticated: true, user: null }))
 *   .failWith(ConnectionService.method.deleteSession, Code.PermissionDenied);
 *
 * const transport = backend.transport();
 * const client = createClient(TokenService, transport);
 *
 * // Assert on recorded requests:
 * const calls = backend.callsTo(TokenService.method.generateToken);
 * expect(calls[0].room).toBe("my-room");
 * ```
 */

import {
  createRouterTransport,
  ConnectError,
  Code,
  type Transport,
  type ServiceImpl,
  type MethodImpl,
  type Interceptor,
} from "@connectrpc/connect";
import type {
  DescService,
  DescMethod,
  DescMethodUnary,
  DescMethodServerStreaming,
  MessageShape,
} from "@bufbuild/protobuf";

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/** A recorded unary call: the raw request message plus metadata. */
export interface RecordedUnaryCall<M extends DescMethodUnary> {
  readonly request: MessageShape<M["input"]>;
  readonly methodName: string;
  readonly serviceName: string;
}

/**
 * A rule for a service that offers one operation both as a unary call and as a server stream whose
 * last event is the unary answer: a backend that serves the unary method but not the stream answers
 * the stream by running the unary handler and emitting `toEvent(answer)` as the only event.
 */
export interface ServerStreamFallback<
  U extends DescMethodUnary = DescMethodUnary,
  S extends DescMethodServerStreaming = DescMethodServerStreaming,
> {
  readonly unary: U;
  readonly stream: S;
  readonly toEvent: (answer: MessageShape<U["output"]>) => MessageShape<S["output"]>;
}

const serverStreamFallbacks: ServerStreamFallback[] = [];

/**
 * Register a {@link ServerStreamFallback} for every backend built afterwards (process-wide). An
 * explicit implementation of the stream method on a backend always wins over the fallback.
 */
export function registerServerStreamFallback<
  U extends DescMethodUnary,
  S extends DescMethodServerStreaming,
>(fallback: ServerStreamFallback<U, S>): void {
  serverStreamFallbacks.push(fallback as unknown as ServerStreamFallback);
}

// Internal accumulator for service/method registrations
type ServiceEntry = { service: DescService; impl: Partial<ServiceImpl<DescService>> };
type MethodEntry = { method: DescMethod; impl: MethodImpl<DescMethod> };

// ---------------------------------------------------------------------------
// Builder
// ---------------------------------------------------------------------------

/**
 * A fluent builder that accumulates ConnectRPC service implementations and
 * produces an in-memory `Transport`.
 *
 * Call `.transport()` once to materialise the transport — subsequent calls
 * return the **same** instance so it is safe to pass to multiple callers.
 */
export class InMemoryRpcBackend {
  private readonly services: ServiceEntry[] = [];
  private readonly methods: MethodEntry[] = [];
  /** requestKey → recorded request messages */
  private readonly recorded = new Map<string, unknown[]>();
  private cachedTransport: Transport | null = null;

  // ---------------------------------------------------------------------------
  // Fluent API
  // ---------------------------------------------------------------------------

  /**
   * Register a full service implementation.
   *
   * You don't have to implement every method — unregistered methods respond
   * with `Code.Unimplemented` (the router default).
   *
   * Returns `this` so calls can be chained.
   */
  implement<T extends DescService>(
    service: T,
    impl: Partial<ServiceImpl<T>>,
  ): this {
    this.services.push({ service, impl: impl as Partial<ServiceImpl<DescService>> });
    return this;
  }

  /**
   * Register a single unary method stub.
   *
   * Convenience shorthand: equivalent to calling `.implement(service, { method: fn })`.
   * The handler receives the request message and must return the response shape.
   *
   * Returns `this` so calls can be chained.
   */
  onUnary<M extends DescMethodUnary>(
    method: M,
    handler: (req: MessageShape<M["input"]>) => MessageShape<M["output"]> | Promise<MessageShape<M["output"]>>,
  ): this {
    this.methods.push({
      method,
      impl: handler as unknown as MethodImpl<DescMethod>,
    });
    return this;
  }

  /**
   * Register a method that always rejects with a `ConnectError`.
   *
   * Returns `this` so calls can be chained.
   */
  failWith(method: DescMethod, code: Code, message?: string): this {
    this.methods.push({
      method,
      impl: ((_req: unknown) => {
        throw new ConnectError(message ?? `[testkit] stubbed failure: ${method.name}`, code);
      }) as unknown as MethodImpl<DescMethod>,
    });
    return this;
  }

  // ---------------------------------------------------------------------------
  // Transport
  // ---------------------------------------------------------------------------

  /**
   * Build (or return the cached) in-memory `Transport`.
   *
   * Once called, the builder is frozen — further calls to `.implement()`,
   * `.onUnary()`, or `.failWith()` after this point will have no effect on
   * the returned transport.  Call `.transport()` after configuring all stubs.
   */
  transport(): Transport {
    if (this.cachedTransport) return this.cachedTransport;

    const recording = this.recorded;
    const recordingInterceptor: Interceptor = (next) => async (req) => {
      // Record unary request messages.  Streaming messages are not captured in
      // v1 — the stream itself is passed through unchanged.
      if (!req.stream) {
        const key = requestKey(req.service.typeName, req.method.name);
        const bucket = recording.get(key) ?? [];
        bucket.push(req.message);
        recording.set(key, bucket);
      }
      return next(req);
    };

    const services = [...this.services];
    const methods = [...this.methods];

    const streamFallbacks = serverStreamFallbacks.flatMap((fallback) => {
      const unaryImpl = findImpl(services, methods, fallback.unary);
      if (unaryImpl === undefined || findImpl(services, methods, fallback.stream) !== undefined) {
        return [];
      }
      return [{ fallback, unaryImpl }];
    });

    this.cachedTransport = createRouterTransport(
      (router) => {
        for (const { service, impl } of services) {
          router.service(service, impl);
        }
        for (const { method, impl } of methods) {
          router.rpc(method, impl);
        }
        for (const { fallback, unaryImpl } of streamFallbacks) {
          router.rpc(fallback.stream, async function* (req: unknown, ctx: unknown) {
            // The unary handler is called directly, so the call is recorded here as the unary
            // interceptor would have; a thrown ConnectError becomes the stream's error.
            const key = requestKey(fallback.unary.parent.typeName, fallback.unary.name);
            const bucket = recording.get(key) ?? [];
            bucket.push(req);
            recording.set(key, bucket);
            const answer = await (unaryImpl as (r: unknown, c: unknown) => unknown)(req, ctx);
            yield fallback.toEvent(answer as never);
          } as unknown as MethodImpl<DescMethod>);
        }
      },
      {
        transport: {
          interceptors: [recordingInterceptor],
        },
      },
    );

    return this.cachedTransport;
  }

  // ---------------------------------------------------------------------------
  // Assertions
  // ---------------------------------------------------------------------------

  /**
   * Return recorded unary request messages for the given method.
   *
   * Use this in assertions after exercising the component / hook under test:
   *
   * ```ts
   * const calls = backend.callsTo(TokenService.method.generateToken);
   * expect(calls).toHaveLength(1);
   * expect(calls[0].room).toBe("test-room");
   * ```
   */
  callsTo<M extends DescMethodUnary>(
    method: M,
  ): Array<MessageShape<M["input"]>> {
    const key = requestKey(method.parent.typeName, method.name);
    return (this.recorded.get(key) ?? []) as Array<MessageShape<M["input"]>>;
  }

  /**
   * Clear all recorded calls.  Useful when a test wants to reset between
   * interactions without constructing a new backend.
   */
  resetCalls(): this {
    this.recorded.clear();
    return this;
  }
}

// ---------------------------------------------------------------------------
// Factory
// ---------------------------------------------------------------------------

/**
 * Create a new in-memory ConnectRPC backend.
 *
 * @example
 * ```ts
 * const backend = anInMemoryRpcBackend()
 *   .implement(TokenService, { generateToken: async () => ({ token: "tok", ttlSeconds: 3600n }) });
 *
 * const transport = backend.transport();
 * ```
 */
export function anInMemoryRpcBackend(): InMemoryRpcBackend {
  return new InMemoryRpcBackend();
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/** The handler registered for `method`, by `.onUnary` / `.failWith` or inside an `.implement`. */
function findImpl(
  services: ServiceEntry[],
  methods: MethodEntry[],
  method: DescMethod,
): unknown {
  const registered = methods.find((entry) => entry.method === method);
  if (registered !== undefined) return registered.impl;
  const service = services.find((entry) => entry.service === method.parent);
  return (service?.impl as Record<string, unknown> | undefined)?.[method.localName];
}

function requestKey(serviceTypeName: string, methodName: string): string {
  return `${serviceTypeName}/${methodName}`;
}
