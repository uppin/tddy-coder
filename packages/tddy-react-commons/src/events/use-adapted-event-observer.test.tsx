import { renderHook } from '@testing-library/react';
import type { Observer } from 'rxjs';
import { describe, expect, it, vi } from 'bun:test';

import { useAdaptedEventObserver, type Envelope } from './index';

interface ParentEvents {
  upstream: string;
}
interface ChildEvents {
  child: number;
  ignored: boolean;
}

describe('useAdaptedEventObserver', () => {
  it('drops child events when adaptation returns void or null', () => {
    const upstreamNext = vi.fn();
    const upstream = { next: upstreamNext } as unknown as Observer<Envelope<ParentEvents>>;

    const { result } = renderHook(() =>
      useAdaptedEventObserver<ParentEvents, ChildEvents>(upstream, envelope => {
        if (envelope.type === 'child') {
          return { type: 'upstream', payload: String(envelope.payload) };
        }
        return null;
      }),
    );

    result.current.next?.({ type: 'ignored', payload: true });
    result.current.next?.({ type: 'child', payload: 7 });

    expect(upstreamNext).toHaveBeenCalledTimes(1);
    expect(upstreamNext).toHaveBeenCalledWith({ type: 'upstream', payload: '7' });
  });

  it('dispatches latest adaptation function without observer identity churn', () => {
    const upstreamNext = vi.fn();
    const upstream = { next: upstreamNext } as unknown as Observer<Envelope<ParentEvents>>;

    const { result, rerender } = renderHook(
      ({ prefix }: { prefix: string }) =>
        useAdaptedEventObserver<ParentEvents, ChildEvents>(upstream, envelope => ({
          type: 'upstream',
          payload: `${prefix}:${envelope.payload}`,
        })),
      { initialProps: { prefix: 'a' } },
    );

    const firstObserver = result.current;
    rerender({ prefix: 'b' });
    expect(result.current).toBe(firstObserver);

    result.current.next?.({ type: 'child', payload: 3 });

    expect(upstreamNext).toHaveBeenCalledWith({ type: 'upstream', payload: 'b:3' });
  });
});
