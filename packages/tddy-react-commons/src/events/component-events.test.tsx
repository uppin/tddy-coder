import { render, screen, fireEvent, act } from '@testing-library/react';
import type { Observer } from 'rxjs';
import { describe, expect, it } from 'bun:test';

import { withEvents, useEventObserver, type RC, type Envelope } from './index';

interface TwinkleEvents {
  exit: { code: number };
  callStarted: null;
  line: { lineNumber: number | undefined; from?: string | null; to?: string | null };
}

interface TwinkleProps {
  label: string;
}

const TwinkleImpl: RC<TwinkleProps, TwinkleEvents> = ({ events, label }) => (
  <div>
    <span>{label}</span>
    <button
      type='button'
      data-testid='emit-exit'
      onClick={() => events.next({ type: 'exit', payload: { code: 0 } })}
    >
      exit
    </button>
    <button
      type='button'
      data-testid='emit-call-started'
      onClick={() => events.next({ type: 'callStarted', payload: null })}
    >
      call
    </button>
    <button type='button' data-testid='emit-complete' onClick={() => events.complete()}>
      complete
    </button>
    <button type='button' data-testid='emit-error' onClick={() => events.error(new Error('boom'))}>
      error
    </button>
    <button
      type='button'
      data-testid='emit-after-terminal'
      onClick={() => events.next({ type: 'exit', payload: { code: 99 } })}
    >
      after
    </button>
  </div>
);

describe('tddy-react-commons/events acceptance', () => {
  let Twinkle: ReturnType<typeof withEvents<TwinkleProps, TwinkleEvents>>;

  beforeEach(() => {
    Twinkle = withEvents<TwinkleProps, TwinkleEvents>(TwinkleImpl);
  });
  it('emits typed event payloads from child to parent observer via handler map', () => {
    const exitPayloads: Array<{ code: number }> = [];

    const Parent = () => {
      const observer = useEventObserver<TwinkleEvents>({
        exit: payload => exitPayloads.push(payload),
      });
      return <Twinkle label='twinkle' eventObserver={observer} />;
    };

    render(<Parent />);
    fireEvent.click(screen.getByTestId('emit-exit'));

    expect(exitPayloads).toEqual([{ code: 0 }]);
  });

  it('subscribes observer on mount and stops delivery after unmount', () => {
    const received: Envelope<TwinkleEvents>[] = [];
    let capturedEvents: Observer<Envelope<TwinkleEvents>> | null = null;

    const CaptureImpl: RC<
      { onCapture: (events: Observer<Envelope<TwinkleEvents>>) => void },
      TwinkleEvents
    > = ({ events, onCapture }) => {
      onCapture(events);
      return null;
    };
    const Capture = withEvents(CaptureImpl);

    const Parent = ({ show }: { show: boolean }) => {
      const observer = useEventObserver<TwinkleEvents>({
        exit: () => received.push({ type: 'exit', payload: { code: 1 } }),
      });
      if (!show) return null;
      return (
        <Capture
          onCapture={events => {
            capturedEvents = events;
          }}
          eventObserver={observer}
        />
      );
    };

    const { rerender } = render(<Parent show={true} />);
    expect(capturedEvents).not.toBeNull();

    act(() => {
      capturedEvents!.next({ type: 'exit', payload: { code: 1 } });
    });
    expect(received).toHaveLength(1);

    rerender(<Parent show={false} />);

    act(() => {
      capturedEvents!.next({ type: 'exit', payload: { code: 2 } });
    });
    expect(received).toHaveLength(1);
  });

  it('renders and emits without an eventObserver without throwing', () => {
    const Parent = () => <Twinkle label='solo' />;
    render(<Parent />);
    fireEvent.click(screen.getByTestId('emit-exit'));
    expect(screen.getByText('solo')).toBeDefined();
  });

  it('dispatches latest handlers without resubscribing', () => {
    const received: number[] = [];
    let observerRef: ReturnType<typeof useEventObserver<TwinkleEvents>> | null = null;

    const Parent = ({ multiplier }: { multiplier: number }) => {
      const observer = useEventObserver<TwinkleEvents>({
        exit: ({ code }) => received.push(code * multiplier),
      });
      observerRef = observer;
      return <Twinkle label='m' eventObserver={observer} />;
    };

    const { rerender } = render(<Parent multiplier={2} />);
    const firstObserver = observerRef;
    rerender(<Parent multiplier={10} />);
    expect(observerRef).toBe(firstObserver);

    fireEvent.click(screen.getByTestId('emit-exit'));
    expect(received).toEqual([0]);
  });

  it('delivers null-payload events emitted without a payload argument', () => {
    let callStartedCount = 0;

    const NullPayloadImpl: RC<object, TwinkleEvents> = ({ events }) => (
      <button
        type='button'
        data-testid='emit-null'
        onClick={() => events.next({ type: 'callStarted', payload: null })}
      >
        null
      </button>
    );
    const NullPayload = withEvents(NullPayloadImpl);

    const Parent = () => {
      const observer = useEventObserver<TwinkleEvents>({
        callStarted: () => {
          callStartedCount += 1;
        },
      });
      return <NullPayload eventObserver={observer} />;
    };

    render(<Parent />);
    fireEvent.click(screen.getByTestId('emit-null'));
    expect(callStartedCount).toBe(1);
  });

  it('ignores child complete/error calls and keeps delivering subsequent events', () => {
    const exitCodes: number[] = [];

    const Parent = () => {
      const observer = useEventObserver<TwinkleEvents>({
        exit: ({ code }) => exitCodes.push(code),
      });
      return <Twinkle label='harden' eventObserver={observer} />;
    };

    render(<Parent />);
    fireEvent.click(screen.getByTestId('emit-complete'));
    fireEvent.click(screen.getByTestId('emit-error'));
    fireEvent.click(screen.getByTestId('emit-after-terminal'));
    expect(exitCodes).toEqual([99]);
  });
});
