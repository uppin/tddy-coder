import { render, screen, fireEvent } from '@testing-library/react';
import { StrictMode } from 'react';
import { describe, expect, it } from 'bun:test';

import { withEvents, type RC, type Envelope } from './index';

interface TestEvents {
  ping: string;
}

describe('withEvents', () => {
  it('passes through props to the wrapped component', () => {
    const Impl: RC<{ title: string }, TestEvents> = ({ title }) => <h1>{title}</h1>;
    const Wrapped = withEvents(Impl);

    render(<Wrapped title='hello' />);
    expect(screen.getByText('hello')).toBeDefined();
  });

  it('sets displayName on the wrapped component', () => {
    function Impl(): null {
      return null;
    }
    Impl.displayName = 'Impl';
    const Wrapped = withEvents<object, TestEvents>(Impl as RC<object, TestEvents>);
    expect(Wrapped.displayName).toBe('withEvents(Impl)');
  });

  it('resubscribes when eventObserver identity changes', () => {
    const received: string[] = [];

    const Impl: RC<object, TestEvents> = ({ events }) => (
      <button type='button' onClick={() => events.next({ type: 'ping', payload: 'x' })}>
        go
      </button>
    );
    const Wrapped = withEvents(Impl);

    const observer1 = { next: (e: Envelope<TestEvents>) => received.push(`1:${e.payload}`) };
    const observer2 = { next: (e: Envelope<TestEvents>) => received.push(`2:${e.payload}`) };

    const { rerender } = render(<Wrapped eventObserver={observer1} />);
    fireEvent.click(screen.getByText('go'));
    rerender(<Wrapped eventObserver={observer2} />);
    fireEvent.click(screen.getByText('go'));

    expect(received).toEqual(['1:x', '2:x']);
  });

  it('delivers events in emission order', () => {
    const order: string[] = [];

    const Impl: RC<object, TestEvents> = ({ events }) => (
      <button
        type='button'
        onClick={() => {
          events.next({ type: 'ping', payload: 'a' });
          events.next({ type: 'ping', payload: 'b' });
        }}
      >
        burst
      </button>
    );
    const Wrapped = withEvents(Impl);

    render(
      <Wrapped
        eventObserver={{
          next: e => order.push(e.payload),
        }}
      />,
    );
    fireEvent.click(screen.getByText('burst'));
    expect(order).toEqual(['a', 'b']);
  });

  it('is safe under StrictMode double mount', () => {
    const received: string[] = [];

    const Impl: RC<object, TestEvents> = ({ events }) => (
      <button type='button' onClick={() => events.next({ type: 'ping', payload: 'ok' })}>
        go
      </button>
    );
    const Wrapped = withEvents(Impl);

    render(
      <StrictMode>
        <Wrapped eventObserver={{ next: e => received.push(e.payload) }} />
      </StrictMode>,
    );
    fireEvent.click(screen.getByText('go'));
    expect(received).toEqual(['ok']);
  });
});
