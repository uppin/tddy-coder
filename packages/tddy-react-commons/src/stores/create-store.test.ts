import { describe, expect, it } from 'bun:test';
import { createStore } from './index';

describe('createStore', () => {
  it('supports setState with an updater function', () => {
    const store = createStore<{ n: number; double: () => void }>(set => ({
      n: 1,
      double: () => set(s => ({ n: s.n * 2 })),
    }));

    store.getState().double();
    expect(store.getState().n).toBe(2);
  });

  it('exposes getState outside React', () => {
    const store = createStore(() => ({ value: 'outside' }));
    expect(store.getState().value).toBe('outside');
  });

  it('ignores error and complete on the Observer channel', () => {
    const store = createStore(() => ({ count: 0 }));
    const observed: number[] = [];
    store.subscribe(s => observed.push(s.count));

    store.error(new Error('fail'));
    store.complete();
    store.setState({ count: 1 });

    expect(store.getState().count).toBe(1);
    expect(observed).toContain(1);
  });
});
