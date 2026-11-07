import { type ReactElement, type ReactNode } from 'react';
import type { Observable, PartialObserver, Subscription } from 'rxjs';

import {
  createAppScope,
  type AppScope,
  type CommandErrorChannel,
  type Request,
  type RequestOptions,
} from '../async/index';
import type { Poller, PollOptions } from '../async/poller';
import { AppContext } from '../container/context';
import {
  createChildEvents,
  type ChildEvents,
  type Envelope,
  type EventMapKeys,
} from '../events/index';
import type { PortMap } from '../ports/index';
import { createStore, type Store } from '../stores/index';

/**
 * Test doubles for the library's own abstractions: a stub context in place of a real handler, a
 * recording `Store`, and the same typed event surface production code uses.
 *
 * Deliberately nothing here doubles a *consumer* pattern. A context's `commands` object, for
 * instance, is a convention this package does not define, so its double belongs with the consumer
 * (see `examples/editor/stub-commands.ts`).
 *
 * Deliberately renderer-agnostic — nothing here imports a testing library, so it works under
 * `@testing-library/react`, Cypress component tests, or anything else.
 */

/**
 * Provides a stub context to the tree under test, standing in for
 * `ReactAppContainer.create(handler, …)`. No handler is started and nothing is torn down.
 *
 * ```tsx
 * render(
 *   <AppContextStub value={{ docStore, status$ }}>
 *     <EditorSidebar />
 *   </AppContextStub>,
 * );
 * ```
 */
export function AppContextStub<Context>({
  value,
  children,
}: {
  value: Context;
  children: ReactNode;
}): ReactElement {
  return <AppContext.Provider value={value}>{children}</AppContext.Provider>;
}

export interface StubStore<T> extends Store<T> {
  /** Every patch passed to `setState`, oldest first. */
  readonly writes: ReadonlyArray<Partial<T>>;
  /** Every whole-state replacement passed to `next`, oldest first. */
  readonly replacements: ReadonlyArray<T>;
  resetWrites(): void;
}

export interface StubStoreOptions<T> {
  /** Called with each `setState` patch (updater results already applied). */
  onSetState?: (patch: Partial<T>) => void;
  /** Called with each whole-state replacement passed to `next`. */
  onNext?: (state: T) => void;
}

/**
 * A real store that additionally records what was written to it, for asserting that an event
 * reached a sink without standing up a handler.
 *
 * The callbacks are the seam for Storybook: pass `onSetState: action('uiStore.setState')` and
 * every sink write shows up in the actions panel.
 */
export function stubStore<T>(initialState: T, options: StubStoreOptions<T> = {}): StubStore<T> {
  const store = createStore<T>(() => initialState);
  const writes: Array<Partial<T>> = [];
  const replacements: T[] = [];

  const { setState, next } = store;

  return Object.assign(store, {
    setState: (partial: Partial<T> | ((state: T) => Partial<T>)) => {
      const patch = typeof partial === 'function' ? partial(store.getState()) : partial;
      writes.push(patch);
      options.onSetState?.(patch);
      setState.call(store, patch);
    },
    next: (state: T) => {
      replacements.push(state);
      options.onNext?.(state);
      next.call(store, state);
    },
    get writes() {
      return writes;
    },
    get replacements() {
      return replacements;
    },
    resetWrites: () => {
      writes.length = 0;
      replacements.length = 0;
    },
  }) as StubStore<T>;
}

export interface EventRecorder<E extends { [K in EventMapKeys<E>]: unknown }> {
  /** Pass as a component's `eventObserver`. */
  readonly observer: PartialObserver<Envelope<E>>;
  /** Every envelope received, oldest first. */
  readonly emitted: ReadonlyArray<Envelope<E>>;
  /** Event names in emission order — handy for asserting sequence. */
  readonly types: ReadonlyArray<keyof E>;
  payloadsOf<K extends keyof E>(type: K): Array<E[K]>;
  lastOf<K extends keyof E>(type: K): E[K] | undefined;
  countOf(type: keyof E): number;
  reset(): void;
}

/**
 * Captures what a component emits, with payloads typed per event name.
 *
 * ```ts
 * const events = eventRecorder<SidebarEvents>();
 * render(<Sidebar eventObserver={events.observer} />);
 * expect(events.lastOf('pageRequested')).toEqual({ page: 3 });
 * ```
 */
export function eventRecorder<E extends { [K in EventMapKeys<E>]: unknown }>(): EventRecorder<E> {
  const emitted: Array<Envelope<E>> = [];

  return {
    observer: {
      next: envelope => {
        emitted.push(envelope);
      },
    },
    get emitted() {
      return emitted;
    },
    get types() {
      return emitted.map(envelope => envelope.type);
    },
    payloadsOf: type =>
      emitted.filter(envelope => envelope.type === type).map(envelope => envelope.payload) as Array<
        E[typeof type]
      >,
    lastOf: type => {
      const matching = emitted.filter(envelope => envelope.type === type);
      return matching[matching.length - 1]?.payload as E[typeof type] | undefined;
    },
    countOf: type => emitted.filter(envelope => envelope.type === type).length,
    reset: () => {
      emitted.length = 0;
    },
  };
}

/**
 * The same typed emitter surface a real child RC receives, pointed at an observer of your
 * choosing — so a test can drive a component's handlers directly:
 *
 * ```ts
 * const events = emittersFor<PageStripEvents>(observer);
 * events.pageClicked({ index: 3 });
 * events.cleared();               // null payload, no argument
 * ```
 *
 * This is the production `createChildEvents`, not a lookalike, so null-payload normalisation
 * and emitter identity behave identically to a mounted child.
 */
export function emittersFor<E extends { [K in EventMapKeys<E>]: unknown }>(
  observer: PartialObserver<Envelope<E>>,
): ChildEvents<E> {
  return createChildEvents<E>(envelope => observer.next?.(envelope));
}

/**
 * A recorder plus emitters over it, for testing a sink or handler map with no component at
 * all: emit through `events`, assert on `recorder` or on a `stubStore`.
 */
export function eventDriver<E extends { [K in EventMapKeys<E>]: unknown }>(
  target?: PartialObserver<Envelope<E>>,
): { events: ChildEvents<E>; recorder: EventRecorder<E> } {
  const recorder = eventRecorder<E>();
  const events = createChildEvents<E>(envelope => {
    recorder.observer.next?.(envelope);
    target?.next?.(envelope);
  });
  return { events, recorder };
}

/**
 * Storybook's `action(name)` — accepted as a parameter so the testkit never imports Storybook.
 */
export type ActionFactory = (name: string) => (...args: unknown[]) => void;

/**
 * Relays every envelope a component emits into a Storybook action, one action per event name:
 *
 * ```tsx
 * import { action } from '@storybook/addon-actions';
 *
 * <ViewOptions eventObserver={actionObserver<ViewOptionsEvents>(action)} />
 * ```
 *
 * Event names come from the envelope at runtime, so there is nothing to declare per event.
 */
export function actionObserver<E extends { [K in EventMapKeys<E>]: unknown }>(
  action: ActionFactory,
  options: { prefix?: string } = {},
): PartialObserver<Envelope<E>> {
  const prefix = options.prefix ?? '';
  return {
    next: envelope => {
      action(`${prefix}${String(envelope.type)}`)(envelope.payload);
    },
  };
}

// ---------------------------------------------------------------------------
// Doubles for the ports and async primitives.
// ---------------------------------------------------------------------------

/**
 * A port map where only what a test cares about is defined.
 *
 * Reaching an unstubbed member throws, naming it. The alternative — `undefined`,
 * or a no-op that returns `undefined` — surfaces two or three frames later as
 * "cannot read property of undefined", pointing at the consumer rather than at
 * the double that was never told what to do.
 */
export function stubPorts<P extends PortMap>(overrides: Partial<P>): P {
  return new Proxy(overrides as P, {
    get(target, prop, receiver) {
      if (typeof prop === 'string' && !(prop in target)) {
        throw new Error(`stubPorts: unstubbed port "${prop}"`);
      }
      return Reflect.get(target, prop, receiver);
    },
  });
}

/** A promise the test settles, so a pending request can be observed *while* pending. */
export interface Deferred<T> {
  readonly promise: Promise<T>;
  resolve(value: T): void;
  reject(error: unknown): void;
}

/**
 * A promise with its settlement handed back.
 *
 * `createRequest`'s loading state exists only between invoke and settle, so a test
 * that returns an already-resolved promise can never see it.
 */
export function deferred<T>(): Deferred<T> {
  let resolve!: (value: T) => void;
  let reject!: (error: unknown) => void;
  const promise = new Promise<T>((res, rej) => {
    resolve = res;
    reject = rej;
  });
  return { promise, resolve, reject };
}

// ---------------------------------------------------------------------------

/** The kinds of resource an `AppScope` builds, as `stubScope` tallies them. */
export type ScopeRegistrationKind = 'request' | 'poller' | 'errorChannel' | 'subscription';

export interface StubScope extends AppScope {
  /** How many of each kind the code under test put in the scope's care. */
  readonly registrations: Readonly<Record<ScopeRegistrationKind, number>>;
  readonly closed: boolean;
  close(): void;
}

/**
 * A scope that also says what was put in it.
 *
 * A handler no longer writes a teardown, so "which resources does this handler
 * hand over to be disposed of?" stopped being answerable by reading one closure.
 * This answers it without a test reaching into a resource's private state.
 *
 * A real scope underneath, not a recording mock: doubles that hand back inert
 * resources let a handler test pass against a handler that never invokes anything.
 */
export function stubScope(): StubScope {
  const scope = createAppScope();
  const registrations: Record<ScopeRegistrationKind, number> = {
    request: 0,
    poller: 0,
    errorChannel: 0,
    subscription: 0,
  };
  let closed = false;

  function tally<T>(kind: ScopeRegistrationKind, resource: T): T {
    registrations[kind] += 1;
    return resource;
  }

  return {
    registrations,

    get closed(): boolean {
      return closed;
    },

    request<Args extends readonly unknown[], T>(
      call: (...args: Args) => Promise<T>,
      options?: RequestOptions,
    ): Request<Args, T> {
      return tally('request', scope.request(call, options));
    },

    poller<T>(call: () => Promise<T>, options: PollOptions<T>): Poller<T> {
      return tally('poller', scope.poller(call, options));
    },

    errorChannel(): CommandErrorChannel {
      return tally('errorChannel', scope.errorChannel());
    },

    subscribe<T>(source: Observable<T>, observer: (value: T) => void): Subscription {
      return tally('subscription', scope.subscribe(source, observer));
    },

    close(): void {
      closed = true;
      scope.close();
    },
  };
}
