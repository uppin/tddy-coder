import { useSyncExternalStore } from 'react';
import { BehaviorSubject, type Observable, type Observer } from 'rxjs';

export { dequal as structurallyEqual } from 'dequal';

export type SetState<T> = (partial: Partial<T> | ((state: T) => Partial<T>)) => void;

export interface Store<T> extends Observable<T>, Observer<T> {
  getState(): T;
  setState(partial: Partial<T> | ((state: T) => Partial<T>)): void;
  useStore<R = T>(selector?: (state: T) => R, equality?: (a: R, b: R) => boolean): R;
}

function applyPartial<T>(partial: Partial<T> | ((state: T) => Partial<T>), state: T): Partial<T> {
  return typeof partial === 'function' ? partial(state) : partial;
}

export function createStore<T>(initializer: (set: SetState<T>, get: () => T) => T): Store<T> {
  let state: T;
  let subject: BehaviorSubject<T> | undefined;

  const setState: SetState<T> = partial => {
    const patch = applyPartial(partial, state);
    state = { ...state, ...patch };
    if (subject) {
      subject.next(state);
    }
  };

  const getState = (): T => state;

  state = initializer(setState, getState);
  subject = new BehaviorSubject<T>(state);

  const useStore = <R = T>(selector?: (s: T) => R, equality?: (a: R, b: R) => boolean): R => {
    const selectorFn = selector ?? ((s: T) => s as unknown as R);
    const equalityFn = equality ?? Object.is;
    const behaviorSubject = subject;

    return useSyncExternalStore(
      onStoreChange => {
        let currentSlice = selectorFn(behaviorSubject.getValue());
        const subscription = behaviorSubject.subscribe(nextState => {
          const nextSlice = selectorFn(nextState);
          if (!equalityFn(currentSlice, nextSlice)) {
            currentSlice = nextSlice;
            onStoreChange();
          }
        });
        return () => subscription.unsubscribe();
      },
      () => selectorFn(behaviorSubject.getValue()),
    );
  };

  const store = Object.assign(subject, {
    getState: () => subject.getValue(),
    setState,
    useStore,
    error: () => {},
    complete: () => {},
  }) as Store<T>;

  return store;
}

/**
 * An action in a slice: takes the store it operates on, plus whatever the caller supplies.
 * Declaring actions store-first keeps them importable, closure-free, and unit-testable with a
 * plain store — no React, no context.
 */
export type StoreAction<T> = (store: Store<T>, ...args: never[]) => unknown;

/** The same actions with the store argument removed, because binding supplied it. */
export type BoundActions<T, S extends Record<string, StoreAction<T>>> = {
  [K in keyof S]: S[K] extends (store: Store<T>, ...args: infer A) => infer R
    ? (...args: A) => R
    : never;
};

/**
 * Bound actions plus reads. Deliberately exposes no `setState` or `next`: a holder can perform
 * the slice's sanctioned writes and nothing else.
 *
 * `use` and `get` are reserved names — an action called either is shadowed, the same way
 * `next`/`error`/`complete` are reserved on `ChildEvents`.
 */
export type SliceHandle<T, S extends Record<string, StoreAction<T>>> = BoundActions<T, S> & {
  /** Selector-based subscription, as `Store.useStore`. */
  use<R = T>(selector?: (state: T) => R, equality?: (a: R, b: R) => boolean): R;
  /** Imperative read, for use outside render. */
  get(): T;
};

/**
 * Binds a slice's actions to a store, so callers stop passing the store as a first argument.
 *
 * ```ts
 * const ui = bindSlice(uiStore, { setDensity: (store, d: Density) => store.setState({ density: d }) });
 * ui.setDensity('compact');
 * ```
 *
 * For React code prefer `createSliceHook` from `tddy-react-commons/container`, which resolves the
 * store from app context. Use this directly in handlers and in tests.
 */
export function bindSlice<T, S extends Record<string, StoreAction<T>>>(
  store: Store<T>,
  actions: S,
): SliceHandle<T, S> {
  const bound: Record<string, unknown> = {};

  for (const [name, action] of Object.entries(actions)) {
    const typedAction = action as (store: Store<T>, ...args: unknown[]) => unknown;
    bound[name] = (...args: unknown[]) => typedAction(store, ...args);
  }

  return Object.assign(bound, {
    use: <R = T>(selector?: (state: T) => R, equality?: (a: R, b: R) => boolean): R =>
      store.useStore(selector, equality),
    get: () => store.getState(),
  }) as SliceHandle<T, S>;
}
