import { renderHook } from '@testing-library/react';
import { describe, expect, it } from 'bun:test';

import { useEventObserver } from './index';

interface Events {
  known: string;
  other: number;
}

describe('useEventObserver', () => {
  it('ignores envelopes for event types without handlers', () => {
    const known: string[] = [];
    const { result } = renderHook(() =>
      useEventObserver<Events>({
        known: payload => known.push(payload),
      }),
    );

    result.current.next?.({ type: 'other', payload: 42 });
    result.current.next?.({ type: 'known', payload: 'hi' });

    expect(known).toEqual(['hi']);
  });

  it('does not throw when next is called for a type with no handler', () => {
    const { result } = renderHook(() => useEventObserver<Events>({}));
    expect(() => result.current.next?.({ type: 'known', payload: 'x' })).not.toThrow();
  });
});
