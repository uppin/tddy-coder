import { render } from '@testing-library/react';
import { useLayoutEffect } from 'react';
import { describe, expect, it } from 'bun:test';

import { withEvents, type RC } from './index';

/**
 * The window between commit and passive-effect flush.
 *
 * React commits the DOM first and flushes passive effects afterwards, so any
 * design that subscribes the parent's observer in a `useEffect` leaves the child
 * mounted, interactive, and emitting into nothing for that interval. Nothing
 * reports it: a `Subject` with no subscriber accepts the value and drops it.
 *
 * This surfaced as an intermittent failure in the editor example — a page click
 * that changed nothing, roughly one full-suite run in five under load, because
 * `waitFor` happened to resolve inside the window. The same thing happens in a
 * browser to a user who clicks a button in the first frame after it appears; it
 * is simply harder to notice.
 *
 * Layout effects run after commit and *before* passive effects, which makes that
 * window addressable from a test instead of load-dependent.
 */

interface ProbeEvents {
  fired: null;
  withPayload: { value: number };
}

const EmitOnLayoutImpl: RC<Record<string, never>, ProbeEvents> = ({ events }) => {
  useLayoutEffect(() => {
    events.fired();
  }, [events]);
  return null;
};

const EmitOnLayout = withEvents(EmitOnLayoutImpl);

const RenderCounterImpl: RC<{ onEvents: (events: unknown) => void }, ProbeEvents> = ({
  onEvents,
  events,
}) => {
  onEvents(events);
  return null;
};

const RenderCounter = withEvents(RenderCounterImpl);

describe('withEvents subscription timing', () => {
  it('delivers an event emitted between commit and passive-effect flush', () => {
    const seen: unknown[] = [];

    render(<EmitOnLayout eventObserver={{ next: envelope => seen.push(envelope) }} />);

    expect(seen).toEqual([{ type: 'fired', payload: null }]);
  });

  it('delivers to the observer the latest render supplied, not the first', () => {
    const first: unknown[] = [];
    const second: unknown[] = [];
    let emit: (() => void) | undefined;
    const Capture: RC<Record<string, never>, ProbeEvents> = ({ events }) => {
      emit = () => events.fired();
      return null;
    };
    const Wrapped = withEvents(Capture);

    const { rerender } = render(
      <Wrapped eventObserver={{ next: envelope => first.push(envelope) }} />,
    );
    rerender(<Wrapped eventObserver={{ next: envelope => second.push(envelope) }} />);
    emit?.();

    expect(first).toEqual([]);
    expect(second).toHaveLength(1);
  });

  it('keeps emitter identity stable across renders', () => {
    // Emitters go into props and dependency arrays; a new object per render would
    // re-run every effect downstream.
    const handed: unknown[] = [];

    const { rerender } = render(<RenderCounter onEvents={events => handed.push(events)} />);
    rerender(<RenderCounter onEvents={events => handed.push(events)} />);

    expect(handed[0]).toBe(handed[1]);
  });

  it('drops an emission when no observer is supplied, without throwing', () => {
    expect(() => render(<EmitOnLayout />)).not.toThrow();
  });

  it('tolerates an observer that implements no next', () => {
    expect(() => render(<EmitOnLayout eventObserver={{}} />)).not.toThrow();
  });
});
