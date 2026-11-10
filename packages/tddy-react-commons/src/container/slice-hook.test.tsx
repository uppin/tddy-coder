import { fireEvent, render, screen } from '@testing-library/react';
import { StrictMode, type FC } from 'react';
import { describe, expect, it } from 'bun:test';

import { bindSlice, createStore, type Store } from '../stores/index';

import { ReactAppContainer, createSliceHook, type AppHandle, type AppHandler } from './index';

interface UiState {
  density: 'compact' | 'comfortable';
  toggles: number;
}

interface TestContext {
  uiStore: Store<UiState>;
}

const initialUiState: UiState = { density: 'comfortable', toggles: 0 };

/** Store-first actions, importable and testable on their own. */
const uiActions = {
  setDensity: (store: Store<UiState>, density: UiState['density']) => store.setState({ density }),
  toggleGrid: (store: Store<UiState>) => store.setState(state => ({ toggles: state.toggles + 1 })),
};

const useUiSlice = createSliceHook({
  select: (context: TestContext) => context.uiStore,
  actions: uiActions,
});

class TestHandler implements AppHandler<TestContext> {
  readonly uiStore = createStore<UiState>(() => initialUiState);

  start(): AppHandle<TestContext> {
    return { context: { uiStore: this.uiStore }, teardown: () => {} };
  }
}

const Panel: FC = () => {
  const ui = useUiSlice();
  const density = ui.use(state => state.density);

  return (
    <>
      <span data-testid='density'>{density}</span>
      <button type='button' onClick={() => ui.setDensity('compact')}>
        compact
      </button>
      <button type='button' onClick={() => ui.toggleGrid()}>
        toggle
      </button>
    </>
  );
};

const renderPanel = () => {
  const handler = new TestHandler();
  return {
    handler,
    ...render(
      ReactAppContainer.create(
        handler,
        <StrictMode>
          <Panel />
        </StrictMode>,
      ),
    ),
  };
};

describe('createSliceHook', () => {
  it('calls actions with no store argument and re-renders the reader', () => {
    const { handler } = renderPanel();

    expect(screen.getByTestId('density').textContent).toBe('comfortable');

    fireEvent.click(screen.getByText('compact'));

    expect(handler.uiStore.getState().density).toBe('compact');
    expect(screen.getByTestId('density').textContent).toBe('compact');
  });

  it('supports updater-style actions', () => {
    const { handler } = renderPanel();

    fireEvent.click(screen.getByText('toggle'));
    fireEvent.click(screen.getByText('toggle'));

    expect(handler.uiStore.getState().toggles).toBe(2);
  });

  it('returns a referentially stable handle across renders, so it is dependency-safe', () => {
    const handles: unknown[] = [];

    const Probe: FC = () => {
      handles.push(useUiSlice());
      return null;
    };

    const handler = new TestHandler();
    const { rerender } = render(ReactAppContainer.create(handler, <Probe />));
    rerender(ReactAppContainer.create(handler, <Probe />));

    expect(handles.length).toBeGreaterThan(1);
    expect(new Set(handles).size).toBe(1);
  });

  it('gives separate handler instances separate handles', () => {
    const seen: unknown[] = [];

    const Probe: FC = () => {
      seen.push(useUiSlice());
      return null;
    };

    render(ReactAppContainer.create(new TestHandler(), <Probe />));
    render(ReactAppContainer.create(new TestHandler(), <Probe />));

    expect(new Set(seen).size).toBe(2);
  });

  it('exposes no store-mutating API on the handle', () => {
    let handle: Record<string, unknown> | undefined;

    const Probe: FC = () => {
      handle = useUiSlice() as unknown as Record<string, unknown>;
      return null;
    };
    render(ReactAppContainer.create(new TestHandler(), <Probe />));

    // The whole point: a holder can perform the slice's writes and nothing else.
    expect(Object.keys(handle!).sort()).toEqual(['get', 'setDensity', 'toggleGrid', 'use']);
    expect(handle).not.toHaveProperty('setState');
    expect(handle).not.toHaveProperty('next');
  });

  it('reads imperatively via get(), outside render', () => {
    let handle: ReturnType<typeof useUiSlice> | undefined;

    const Probe: FC = () => {
      handle = useUiSlice();
      return null;
    };
    render(ReactAppContainer.create(new TestHandler(), <Probe />));

    handle!.setDensity('compact');

    expect(handle!.get().density).toBe('compact');
  });
});

describe('bindSlice without React', () => {
  it('binds actions to a store with no renderer involved', () => {
    const store = createStore<UiState>(() => initialUiState);
    const ui = bindSlice(store, uiActions);

    ui.setDensity('compact');
    ui.toggleGrid();

    expect(store.getState()).toEqual({ density: 'compact', toggles: 1 });
    expect(ui.get().density).toBe('compact');
  });

  it('leaves the underlying store untouched by the handle it produces', () => {
    const store = createStore<UiState>(() => initialUiState);
    bindSlice(store, uiActions);

    expect(store.getState()).toEqual(initialUiState);
  });
});
