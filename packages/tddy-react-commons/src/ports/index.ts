import { createAppScope, type AppScope, type ScopedAppResources } from '../async/index';
import type { AppHandler } from '../container/index';

export type { AppScope } from '../async/index';

/**
 * Driven adapters, keyed by the name the handler knows them by.
 *
 * A bag of `unknown` rather than a base interface every port must extend: a port
 * is defined by the consumer's own contract, and requiring it to implement one of
 * ours would be a dependency pointing the wrong way through the hexagon.
 */
export type PortMap = Record<string, unknown>;

/**
 * Business logic that is *given* its adapters instead of constructing them.
 *
 * The container takes an `AppHandler`, whose `start()` has no parameters, so
 * without this a handler either constructs its own adapters — the thing a hexagon
 * exists to prevent — or takes them through a constructor, which is a convention
 * nothing checks. `PortedAppHandler` makes "the adapters arrive from outside" a
 * fact of the type, and `withPorts` is the single place the two meet.
 *
 * The scope arrives the same way and for the same reason: a handler that built its
 * own long-lived resources would also have to reverse each of them by hand, in a
 * teardown closure nothing checks against the list above it.
 */
export interface PortedAppHandler<P extends PortMap, Context> {
  start(ports: P, scope: AppScope): PortedAppHandle<Context>;
}

/**
 * What a ported handler returns.
 *
 * No `teardown`, unlike `AppHandle`: everything closeable came from the scope, so
 * there is nothing left for the handler to reverse. `withPorts` supplies the
 * `teardown` the container requires.
 */
export interface PortedAppHandle<Context> {
  context: Context;
}

/**
 * The scope as a handler sees it — the four factories, and no `close`.
 *
 * A fresh object rather than the scope itself, so the restriction holds at runtime
 * and not only in the type: `close` is simply not there to be reached for.
 */
function handedTo({ request, poller, errorChannel, subscribe }: ScopedAppResources): AppScope {
  return { request, poller, errorChannel, subscribe };
}

/** The port map a `PortedAppHandler` requires. */
export type PortsOf<H> = H extends PortedAppHandler<infer P, unknown> ? P : never;

/** The context a `PortedAppHandler` provides. */
export type ContextOf<H> = H extends PortedAppHandler<PortMap, infer Context> ? Context : never;

const handlerCache = new WeakMap<
  PortedAppHandler<PortMap, unknown>,
  Map<PortMap, AppHandler<unknown>>
>();

/**
 * Bind adapters to a handler, giving the `AppHandler` the container accepts.
 *
 * Identity is stable for a given `(handler, ports)` pair, because the container
 * restarts on handler identity change — a fresh object per render would tear the
 * app down and start it again on every commit. The scope is therefore built inside
 * `start`, not beside it: this object outlives a mount, and a scope built out here
 * would be closed by the first unmount and reused, already dead, by the next.
 */
export function withPorts<P extends PortMap, Context>(
  handler: PortedAppHandler<P, Context>,
  ports: P,
): AppHandler<Context> {
  let portsByHandler = handlerCache.get(handler);
  if (!portsByHandler) {
    portsByHandler = new Map();
    handlerCache.set(handler, portsByHandler);
  }

  let appHandler = portsByHandler.get(ports);
  if (!appHandler) {
    appHandler = {
      start: () => {
        const scope = createAppScope();
        try {
          const { context } = handler.start(ports, handedTo(scope));
          return { context, teardown: () => scope.close() };
        } catch (error) {
          // The scope is reachable only through the handle a successful `start`
          // returns, so a handler that registers a poller and then throws would
          // leave it running with nothing left holding a reference to close it.
          // The container never sees a handle for a failed start, and therefore
          // never calls a teardown.
          scope.close();
          throw error;
        }
      },
    };
    portsByHandler.set(ports, appHandler);
  }

  return appHandler as AppHandler<Context>;
}
