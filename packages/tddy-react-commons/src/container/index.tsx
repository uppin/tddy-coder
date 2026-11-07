// TODO(react-commons): implement — contract surface; bodies land with the implementation.
import { type ReactElement, type ReactNode } from 'react';
import { type SliceHandle, type Store, type StoreAction } from '../stores/index';
/**
 * Everything a started app exposes: the value handed to the React tree, and the
 * teardown that reverses `start()`. The container owns `teardown`; the view layer
 * only ever sees `context`.
 */
export interface AppHandle<Context> {
    context: Context;
    teardown(): void;
}
/**
 * Contract for React-free business logic. `start()` is the only member: context
 * cannot exist before the app is running, so there is no unstarted state to read.
 */
export interface AppHandler<Context> {
    start(): AppHandle<Context>;
}
export function useAppContext<Context>(): Context {
    throw new Error("TODO(react-commons): implement");
}
export const ReactAppContainer: {
    create<Context>(handler: AppHandler<Context>, appTree: ReactNode): ReactElement;
} = new Proxy({}, NOT_IMPLEMENTED) as unknown as {
    create<Context>(handler: AppHandler<Context>, appTree: ReactNode): ReactElement;
};
/**
 * Config for a slice hook: where the store lives in app context, and the slice's actions.
 */
export interface SliceHookConfig<Context, State, Actions extends Record<string, StoreAction<State>>> {
    select: (context: Context) => Store<State>;
    actions: Actions;
}
/**
 * Builds a hook for one slice — the Redux-Toolkit shape, adapted to this container.
 *
 * A slice declares its actions store-first, once, at module scope. The hook resolves the store
 * from app context and hands back the actions already bound, so component code never touches the
 * store or passes it as an argument:
 *
 * ```ts
 * // ui-slice.ts
 * export const useUiSlice = createSliceHook({
 *   select: (context: EditorAppContext) => context.uiStore,
 *   actions: {
 *     setDensity: (store, density: Density) => store.setState({ density }),
 *     toggleGrid: store => store.setState(state => ({ showGrid: !state.showGrid })),
 *   },
 * });
 *
 * // any component
 * const ui = useUiSlice();
 * const density = ui.use(state => state.density);
 * ui.setDensity('compact');
 * ```
 *
 * The handle is cached per store instance in a `WeakMap`, so it is referentially stable across
 * renders and components — safe in a dependency array — while still isolating separate handler
 * instances from each other, which matters for tests and multiple roots.
 *
 * Reads are opt-in: calling the hook subscribes to nothing until you call `use`.
 */
export function createSliceHook<Context, State, Actions extends Record<string, StoreAction<State>>>({ select, actions, }: SliceHookConfig<Context, State, Actions>): () => SliceHandle<State, Actions> {
    throw new Error("TODO(react-commons): implement");
}

const NOT_IMPLEMENTED: ProxyHandler<object> = {
  get() {
    throw new Error("TODO(react-commons): implement");
  },
};
