import { act, render, screen } from '@testing-library/react';
import { StrictMode, type FC } from 'react';
import { describe, expect, it } from 'bun:test';

import { createStore, type Store } from '../stores/index';

import { ReactAppContainer, useAppContext, type AppHandle, type AppHandler } from './index';

interface CounterState {
  count: number;
  increment: () => void;
}

interface CounterContext {
  label: string;
  counterStore: Store<CounterState>;
}

class CounterHandler implements AppHandler<CounterContext> {
  startCount = 0;
  teardownCount = 0;

  readonly counterStore = createStore<CounterState>(set => ({
    count: 0,
    increment: () => set(state => ({ count: state.count + 1 })),
  }));

  constructor(private readonly label = 'primary') {}

  get liveHandles(): number {
    return this.startCount - this.teardownCount;
  }

  start(): AppHandle<CounterContext> {
    this.startCount += 1;
    return {
      context: { label: this.label, counterStore: this.counterStore },
      teardown: () => {
        this.teardownCount += 1;
      },
    };
  }
}

const useCounterContext = () => useAppContext<CounterContext>();

const CountLabel: FC = () => {
  const { label, counterStore } = useCounterContext();
  const count = counterStore.useStore(state => state.count);
  return <span data-testid='count'>{`${label}:${count}`}</span>;
};

describe('ReactAppContainer.create', () => {
  it('provides the handler context to a nested component (AT2)', () => {
    const handler = new CounterHandler();

    render(ReactAppContainer.create(handler, <CountLabel />));

    expect(screen.getByTestId('count').textContent).toBe('primary:0');
    expect(handler.startCount).toBe(1);
  });

  it('re-renders the tree when the handler mutates a context store (AT2)', () => {
    const handler = new CounterHandler();
    render(ReactAppContainer.create(handler, <CountLabel />));

    act(() => {
      handler.counterStore.getState().increment();
    });

    expect(screen.getByTestId('count').textContent).toBe('primary:1');
  });

  it('mounts the app tree only after the handler has started', () => {
    const handler = new CounterHandler();
    const startCountsAtRender: number[] = [];

    const Probe: FC = () => {
      startCountsAtRender.push(handler.startCount);
      return null;
    };

    render(ReactAppContainer.create(handler, <Probe />));

    expect(startCountsAtRender.length).toBeGreaterThan(0);
    expect(Math.min(...startCountsAtRender)).toBeGreaterThanOrEqual(1);
  });

  it('tears down the handle exactly once on unmount', () => {
    const handler = new CounterHandler();
    const { unmount } = render(ReactAppContainer.create(handler, <CountLabel />));

    expect(handler.teardownCount).toBe(0);
    unmount();

    expect(handler.teardownCount).toBe(1);
    expect(handler.liveHandles).toBe(0);
  });

  it('leaves exactly one live handle under StrictMode double mount (AT3)', () => {
    const handler = new CounterHandler();

    const { unmount } = render(
      <StrictMode>{ReactAppContainer.create(handler, <CountLabel />)}</StrictMode>,
    );

    expect(handler.liveHandles).toBe(1);
    expect(handler.teardownCount).toBe(handler.startCount - 1);
    expect(screen.getByTestId('count').textContent).toBe('primary:0');

    unmount();
    expect(handler.liveHandles).toBe(0);
  });

  it('starts once when StrictMode is nested inside the app tree (AT3)', () => {
    const handler = new CounterHandler();

    render(
      ReactAppContainer.create(
        handler,
        <StrictMode>
          <CountLabel />
        </StrictMode>,
      ),
    );

    expect(handler.startCount).toBe(1);
    expect(handler.liveHandles).toBe(1);
    expect(screen.getByTestId('count').textContent).toBe('primary:0');
  });

  it('keeps the handle across unrelated re-renders', () => {
    const handler = new CounterHandler();
    const { rerender } = render(ReactAppContainer.create(handler, <CountLabel />));

    rerender(ReactAppContainer.create(handler, <CountLabel />));

    expect(handler.startCount).toBe(1);
    expect(handler.teardownCount).toBe(0);
  });

  it('tears down the previous handle when the handler identity changes', () => {
    const first = new CounterHandler('first');
    const second = new CounterHandler('second');

    const { rerender } = render(ReactAppContainer.create(first, <CountLabel />));
    rerender(ReactAppContainer.create(second, <CountLabel />));

    expect(first.teardownCount).toBe(1);
    expect(first.liveHandles).toBe(0);
    expect(second.liveHandles).toBe(1);
    expect(screen.getByTestId('count').textContent).toBe('second:0');
  });
});
