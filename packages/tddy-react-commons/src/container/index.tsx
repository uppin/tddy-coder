import { useContext, useEffect, useState, type ReactElement, type ReactNode } from 'react';

import { bindSlice, type SliceHandle, type Store, type StoreAction } from '../stores/index';

import { AppContext, MISSING_CONTEXT } from './context';

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
  const value = useContext(AppContext);

  if (value === MISSING_CONTEXT) {
    throw new Error(
      'useAppContext() was called outside an app tree. Render the element returned by ReactAppContainer.create(handler, appTree).',
    );
  }

  return value as Context;
}

interface AppContainerProps<Context> {
  handler: AppHandler<Context>;
  children: ReactNode;
}

function AppContainer<Context>({
  handler,
  children,
}: AppContainerProps<Context>): ReactElement | null {
  const [handle, setHandle] = useState<AppHandle<Context> | null>(null);

  useEffect(() => {
    const started = handler.start();
    setHandle(started);

    return () => {
      setHandle(current => (current === started ? null : current));
      started.teardown();
    };
  }, [handler]);

  // Gate the tree until the handle exists. Starting in an effect (rather than during
  // render) is what keeps every start paired with a teardown under StrictMode; the
  // cost is that the tree mounts one commit late.
  if (!handle) {
    return null;
  }

  return <AppContext.Provider value={handle.context}>{children}</AppContext.Provider>;
}

AppContainer.displayName = 'ReactAppContainer';

export const ReactAppContainer = {
  create<Context>(handler: AppHandler<Context>, appTree: ReactNode): ReactElement {
    return <AppContainer handler={handler}>{appTree}</AppContainer>;
  },
};

/**
 * Config for a slice hook: where the store lives in app context, and the slice's actions.
 */
export interface SliceHookConfig<
  Context,
  State,
  Actions extends Record<string, StoreAction<State>>,
> {
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
export function createSliceHook<
  Context,
  State,
  Actions extends Record<string, StoreAction<State>>,
>({
  select,
  actions,
}: SliceHookConfig<Context, State, Actions>): () => SliceHandle<State, Actions> {
  const handles = new WeakMap<Store<State>, SliceHandle<State, Actions>>();

  return function useSlice(): SliceHandle<State, Actions> {
    const store = select(useAppContext<Context>());

    let handle = handles.get(store);
    if (!handle) {
      handle = bindSlice(store, actions);
      handles.set(store, handle);
    }
    return handle;
  };
}
