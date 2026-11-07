// TODO(react-commons): implement — contract surface; bodies land with the implementation.
import { type Observable, type Observer } from 'rxjs';
export { dequal as structurallyEqual } from 'dequal';
export type SetState<T> = (partial: Partial<T> | ((state: T) => Partial<T>)) => void;
export interface Store<T> extends Observable<T>, Observer<T> {
    getState(): T;
    setState(partial: Partial<T> | ((state: T) => Partial<T>)): void;
    useStore<R = T>(selector?: (state: T) => R, equality?: (a: R, b: R) => boolean): R;
}
export function createStore<T>(initializer: (set: SetState<T>, get: () => T) => T): Store<T> {
    throw new Error("TODO(react-commons): implement");
}
/**
 * An action in a slice: takes the store it operates on, plus whatever the caller supplies.
 * Declaring actions store-first keeps them importable, closure-free, and unit-testable with a
 * plain store — no React, no context.
 */
export type StoreAction<T> = (store: Store<T>, ...args: never[]) => unknown;
/** The same actions with the store argument removed, because binding supplied it. */
export type BoundActions<T, S extends Record<string, StoreAction<T>>> = {
    [K in keyof S]: S[K] extends (store: Store<T>, ...args: infer A) => infer R ? (...args: A) => R : never;
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
export function bindSlice<T, S extends Record<string, StoreAction<T>>>(store: Store<T>, actions: S): SliceHandle<T, S> {
    throw new Error("TODO(react-commons): implement");
}
