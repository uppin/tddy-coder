import { render, screen, act } from '@testing-library/react';
import { Subject, map } from 'rxjs';
import { describe, expect, it } from 'bun:test';

import { createStore } from './index';

describe('tddy-react-commons/stores acceptance', () => {
  it('creates a store with colocated actions that merge partial state and expose synchronous getState', () => {
    const store = createStore<{ count: number; increase: (by: number) => void }>(set => ({
      count: 0,
      increase: (by: number) => set(s => ({ count: s.count + by })),
    }));

    expect(store.getState().count).toBe(0);
    store.getState().increase(3);
    expect(store.getState().count).toBe(3);
    store.setState({ count: 10 });
    expect(store.getState().count).toBe(10);
  });

  it('re-renders subscribed components only when the selected slice changes', () => {
    const store = createStore<{
      a: number;
      b: number;
      bumpA: () => void;
      bumpB: () => void;
    }>(set => ({
      a: 0,
      b: 0,
      bumpA: () => set(s => ({ a: s.a + 1 })),
      bumpB: () => set(s => ({ b: s.b + 1 })),
    }));

    type StoreState = ReturnType<typeof store.getState>;

    let aRenderCount = 0;
    let bRenderCount = 0;

    const AViewer = () => {
      const a = store.useStore((s: StoreState) => s.a);
      aRenderCount += 1;
      return <span data-testid='a'>{a}</span>;
    };

    const BViewer = () => {
      const b = store.useStore((s: StoreState) => s.b);
      bRenderCount += 1;
      return <span data-testid='b'>{b}</span>;
    };

    render(
      <>
        <AViewer />
        <BViewer />
      </>,
    );

    const initialA = aRenderCount;
    const initialB = bRenderCount;

    act(() => {
      store.getState().bumpA();
    });

    expect(screen.getByTestId('a').textContent).toBe('1');
    expect(aRenderCount).toBe(initialA + 1);
    expect(bRenderCount).toBe(initialB);
  });

  it('replaces whole state via next and merges via setState', () => {
    const store = createStore<{ count: number; tag: string }>(() => ({
      count: 0,
      tag: 'init',
    }));

    store.setState({ count: 5 });
    expect(store.getState()).toMatchObject({ count: 5, tag: 'init' });

    store.next({ count: 100, tag: 'replaced' });
    expect(store.getState()).toEqual({ count: 100, tag: 'replaced' });
  });

  it('acts as an Observer in an rxjs pipeline', () => {
    const store = createStore<{ value: number }>(() => ({ value: 0 }));
    const observed: number[] = [];

    const subscription = store.subscribe(state => observed.push(state.value));

    const source$ = new Subject<number>();
    source$.pipe(map(n => ({ value: n }))).subscribe(store);
    source$.next(42);

    expect(store.getState().value).toBe(42);
    expect(observed).toContain(42);

    subscription.unsubscribe();
  });
});
