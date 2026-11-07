// TODO(react-commons): implement — contract surface; bodies land with the implementation.
import { type FC, type ReactElement } from 'react';
import type { Observer, PartialObserver } from 'rxjs';
export interface EventMap {
    [eventName: string]: unknown;
}
export type EventMapKeys<E> = keyof E & string;
export type Envelope<E extends {
    [K in EventMapKeys<E>]: unknown;
}> = {
    [K in keyof E]: {
        type: K;
        payload: E[K];
    };
}[keyof E];
export type EventedProps<P, E extends {
    [K in EventMapKeys<E>]: unknown;
}> = P & {
    eventObserver?: PartialObserver<Envelope<E>> | null;
};
/**
 * Per-event emitter methods derived from the event map — no second declaration.
 * A `null`-payload event takes no argument: `events.zoomIn()`.
 */
export type EventEmitters<E extends {
    [K in EventMapKeys<E>]: unknown;
}> = {
    [K in keyof E]: null extends E[K] ? (payload?: E[K]) => void : (payload: E[K]) => void;
};
/**
 * What a child RC receives. Both styles are always available: `events.pageClicked(payload)`
 * for emitting, and `events.next(envelope)` for forwarding an envelope you already hold.
 *
 * Caveat: `next`, `error`, and `complete` are reserved — an event named one of those is
 * shadowed by the observer method and cannot be emitted as a method.
 */
export type ChildEvents<E extends {
    [K in EventMapKeys<E>]: unknown;
}> = Observer<Envelope<E>> & EventEmitters<E>;
export type RC<P, E extends {
    [K in EventMapKeys<E>]: unknown;
}> = (props: P & {
    events: ChildEvents<E>;
}) => ReactElement | null;
export function useEventObserver<E extends {
    [K in EventMapKeys<E>]: unknown;
}>(handlers: Partial<{
    [K in keyof E]: (payload: E[K]) => void;
}>): PartialObserver<Envelope<E>> {
    throw new Error("TODO(react-commons): implement");
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
export function createChildEvents<E extends {
    [K in EventMapKeys<E>]: unknown;
}>(emit: (envelope: Envelope<E>) => void): ChildEvents<E> {
    throw new Error("TODO(react-commons): implement");
}
export function withEvents<P, E extends {
    [K in EventMapKeys<E>]: unknown;
}>(Component: RC<P, E>): FC<EventedProps<P, E>> {
    throw new Error("TODO(react-commons): implement");
}
export function useAdaptedEventObserver<ParentE extends {
    [K in EventMapKeys<ParentE>]: unknown;
}, ChildE extends {
    [K in EventMapKeys<ChildE>]: unknown;
}>(upstreamEvents: Observer<Envelope<ParentE>>, adapt: (envelope: Envelope<ChildE>) => Envelope<ParentE> | void | null): PartialObserver<Envelope<ChildE>> {
    throw new Error("TODO(react-commons): implement");
}
