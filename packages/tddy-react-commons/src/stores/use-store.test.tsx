import { render, act } from '@testing-library/react';
import { describe, expect, it } from 'bun:test';

import { createStore } from './index';

describe('useStore', () => {
  it('uses Object.is as the default equality function', () => {
    const store = createStore(() => ({ ref: { id: 1 }, count: 0 }));

    let renderCount = 0;
    const Viewer = () => {
      const ref = store.useStore(s => s.ref);
      renderCount += 1;
      return ref.id;
    };

    render(<Viewer />);
    const afterMount = renderCount;

    act(() => {
      store.setState({ count: 1 });
    });

    expect(renderCount).toBe(afterMount);
  });

  it('supports a custom equality function', () => {
    const store = createStore(() => ({ value: 0 }));

    let renderCount = 0;
    const Viewer = () => {
      const value = store.useStore(
        s => s.value,
        (a, b) => Math.abs(a - b) < 2,
      );
      renderCount += 1;
      return value;
    };

    render(<Viewer />);
    act(() => store.setState({ value: 1 }));
    expect(renderCount).toBe(1);

    act(() => store.setState({ value: 5 }));
    expect(renderCount).toBe(2);
  });

  it('subscribes to the whole state when no selector is provided', () => {
    const store = createStore(() => ({ a: 1, b: 2 }));

    let snapshot = null as { a: number; b: number } | null;
    const Viewer = () => {
      snapshot = store.useStore();
      return null;
    };

    render(<Viewer />);
    expect(snapshot).toEqual({ a: 1, b: 2 });

    act(() => store.setState({ a: 9 }));
    expect(snapshot).toEqual({ a: 9, b: 2 });
  });
});
