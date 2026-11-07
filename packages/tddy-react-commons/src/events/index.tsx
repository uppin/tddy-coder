import { useEffect, useMemo, useRef, type FC, type ReactElement } from 'react';
import type { Observer, PartialObserver } from 'rxjs';

export interface EventMap {
  [eventName: string]: unknown;
}

export type EventMapKeys<E> = keyof E & string;

export type Envelope<E extends { [K in EventMapKeys<E>]: unknown }> = {
  [K in keyof E]: { type: K; payload: E[K] };
}[keyof E];

export type EventedProps<P, E extends { [K in EventMapKeys<E>]: unknown }> = P & {
  eventObserver?: PartialObserver<Envelope<E>> | null;
};

/**
 * Per-event emitter methods derived from the event map — no second declaration.
 * A `null`-payload event takes no argument: `events.zoomIn()`.
 */
export type EventEmitters<E extends { [K in EventMapKeys<E>]: unknown }> = {
  [K in keyof E]: null extends E[K] ? (payload?: E[K]) => void : (payload: E[K]) => void;
};

/**
 * What a child RC receives. Both styles are always available: `events.pageClicked(payload)`
 * for emitting, and `events.next(envelope)` for forwarding an envelope you already hold.
 *
 * Caveat: `next`, `error`, and `complete` are reserved — an event named one of those is
 * shadowed by the observer method and cannot be emitted as a method.
 */
export type ChildEvents<E extends { [K in EventMapKeys<E>]: unknown }> = Observer<Envelope<E>> &
  EventEmitters<E>;

export type RC<P, E extends { [K in EventMapKeys<E>]: unknown }> = (
  props: P & { events: ChildEvents<E> },
) => ReactElement | null;

export function useEventObserver<E extends { [K in EventMapKeys<E>]: unknown }>(
  handlers: Partial<{ [K in keyof E]: (payload: E[K]) => void }>,
): PartialObserver<Envelope<E>> {
  const handlersRef = useRef(handlers);
  useEffect(() => {
    handlersRef.current = handlers;
  });

  return useMemo<PartialObserver<Envelope<E>>>(
    () => ({
      next: envelope => {
        const handler = handlersRef.current[envelope.type] as
          | ((payload: E[keyof E]) => void)
          | undefined;
        if (handler) {
          handler(envelope.payload as E[keyof E]);
        }
      },
    }),
    [],
  );
}

/**
 * Builds a child `events` object around an `emit` function.
 *
 * A `Proxy` is what makes the method style work without a second declaration: the event
 * names live only in the type, which is erased at runtime, so there is nothing to
 * enumerate and build a plain object from. Emitters are cached per name so
 * `events.pageClicked` keeps a stable identity across accesses and can be passed as a prop
 * or listed in a dependency array.
 *
 * Exported so tests can drive a component through the exact same surface a real child
 * receives, rather than a lookalike (see `tddy-react-commons/testkit`).
 */
export function createChildEvents<E extends { [K in EventMapKeys<E>]: unknown }>(
  emit: (envelope: Envelope<E>) => void,
): ChildEvents<E> {
  const observer: Observer<Envelope<E>> = {
    next: envelope => {
      emit(envelope);
    },
    error: () => {},
    complete: () => {},
  };

  const emitters = new Map<string, (...args: unknown[]) => void>();

  return new Proxy(observer, {
    get(target, property, receiver) {
      if (property in target || typeof property !== 'string') {
        return Reflect.get(target, property, receiver);
      }

      let emitter = emitters.get(property);
      if (!emitter) {
        emitter = (...args: unknown[]) => {
          // No argument means a null-payload event; normalise so the parent receives the
          // `null` its event map declares rather than `undefined`.
          const payload = args.length > 0 ? args[0] : null;
          observer.next({ type: property, payload } as Envelope<E>);
        };
        emitters.set(property, emitter);
      }
      return emitter;
    },
  }) as ChildEvents<E>;
}

export function withEvents<P, E extends { [K in EventMapKeys<E>]: unknown }>(
  Component: RC<P, E>,
): FC<EventedProps<P, E>> {
  const Wrapped: FC<EventedProps<P, E>> = props => {
    const { eventObserver, ...rest } = props;

    // The observer is read through a ref updated during render, not subscribed in
    // an effect. React commits the DOM before passive effects flush, so an effect
    // subscription leaves a window in which the child is mounted, interactive, and
    // emitting into nothing — and a Subject does not replay, so those events are
    // gone. In a test that window is whatever gap the scheduler leaves between
    // commit and flush; in a browser it is a user clicking a button in the first
    // frame after it appears.
    const observerRef = useRef(eventObserver);
    observerRef.current = eventObserver;

    // Delivery still stops at unmount: a captured emitter outliving its component
    // would push into a parent handler whose state is gone. Initialised `true` and
    // re-set on mount rather than only cleared on unmount, because StrictMode runs
    // the effect, its cleanup, and the effect again with no render in between — a
    // cleanup-only version would clear the flag and never restore it.
    const mountedRef = useRef(true);
    useEffect(() => {
      mountedRef.current = true;
      return () => {
        mountedRef.current = false;
      };
    }, []);

    // `[]`, not `[eventObserver]`: emitter identity has to be stable for props and
    // dependency arrays, and the ref already keeps the destination current.
    const events = useMemo(
      () =>
        createChildEvents<E>(envelope => {
          if (mountedRef.current) {
            observerRef.current?.next?.(envelope);
          }
        }),
      [],
    );

    return <Component {...(rest as P)} events={events} />;
  };

  Wrapped.displayName = `withEvents(${
    (Component as { displayName?: string; name?: string }).displayName ??
    Component.name ??
    'Component'
  })`;
  return Wrapped;
}

export function useAdaptedEventObserver<
  ParentE extends { [K in EventMapKeys<ParentE>]: unknown },
  ChildE extends { [K in EventMapKeys<ChildE>]: unknown },
>(
  upstreamEvents: Observer<Envelope<ParentE>>,
  adapt: (envelope: Envelope<ChildE>) => Envelope<ParentE> | void | null,
): PartialObserver<Envelope<ChildE>> {
  const adaptRef = useRef(adapt);
  const upstreamRef = useRef(upstreamEvents);

  useEffect(() => {
    adaptRef.current = adapt;
    upstreamRef.current = upstreamEvents;
  });

  return useMemo<PartialObserver<Envelope<ChildE>>>(
    () => ({
      next: envelope => {
        const parentEnvelope = adaptRef.current(envelope);
        if (parentEnvelope != null) {
          upstreamRef.current.next(parentEnvelope);
        }
      },
    }),
    [],
  );
}
