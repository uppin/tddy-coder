import { fireEvent, render, screen } from '@testing-library/react';
import { StrictMode } from 'react';
import { describe, expect, it } from 'bun:test';

import { useEventObserver, withEvents, type Envelope, type RC } from './index';

interface WidgetEvents {
  pageClicked: { index: number };
  zoomIn: null;
  renamed: string;
}

describe('typed event emitters', () => {
  it('emits via a method named after the event', () => {
    const received: Array<Envelope<WidgetEvents>> = [];

    const Impl: RC<object, WidgetEvents> = ({ events }) => (
      <button type='button' onClick={() => events.pageClicked({ index: 3 })}>
        go
      </button>
    );
    const Wrapped = withEvents(Impl);

    render(<Wrapped eventObserver={{ next: envelope => received.push(envelope) }} />);
    fireEvent.click(screen.getByText('go'));

    expect(received).toEqual([{ type: 'pageClicked', payload: { index: 3 } }]);
  });

  it('takes no argument for a null-payload event and emits null, not undefined', () => {
    const received: Array<Envelope<WidgetEvents>> = [];

    const Impl: RC<object, WidgetEvents> = ({ events }) => (
      <button type='button' onClick={() => events.zoomIn()}>
        zoom
      </button>
    );
    const Wrapped = withEvents(Impl);

    render(<Wrapped eventObserver={{ next: envelope => received.push(envelope) }} />);
    fireEvent.click(screen.getByText('zoom'));

    expect(received).toEqual([{ type: 'zoomIn', payload: null }]);
    expect(received[0]?.payload).toBeNull();
  });

  it('still supports next() with a full envelope', () => {
    const received: Array<Envelope<WidgetEvents>> = [];

    const Impl: RC<object, WidgetEvents> = ({ events }) => (
      <button type='button' onClick={() => events.next({ type: 'renamed', payload: 'x' })}>
        rename
      </button>
    );
    const Wrapped = withEvents(Impl);

    render(<Wrapped eventObserver={{ next: envelope => received.push(envelope) }} />);
    fireEvent.click(screen.getByText('rename'));

    expect(received).toEqual([{ type: 'renamed', payload: 'x' }]);
  });

  it('keeps a stable identity per emitter so it can be a prop or a dep', () => {
    const identities: boolean[] = [];

    const Impl: RC<object, WidgetEvents> = ({ events }) => {
      identities.push(events.pageClicked === events.pageClicked);
      return null;
    };
    const Wrapped = withEvents(Impl);

    render(<Wrapped />);

    expect(identities).toContain(true);
    expect(identities).not.toContain(false);
  });

  it('reaches the parent handler map, which sees the declared payload', () => {
    const pages: number[] = [];
    let zoomed = 0;

    const Impl: RC<object, WidgetEvents> = ({ events }) => (
      <>
        <button type='button' onClick={() => events.pageClicked({ index: 7 })}>
          page
        </button>
        <button type='button' onClick={() => events.zoomIn()}>
          zoom
        </button>
      </>
    );
    const Wrapped = withEvents(Impl);

    const Parent = () => {
      const observer = useEventObserver<WidgetEvents>({
        pageClicked: ({ index }) => pages.push(index),
        zoomIn: () => {
          zoomed += 1;
        },
      });
      return <Wrapped eventObserver={observer} />;
    };

    render(
      <StrictMode>
        <Parent />
      </StrictMode>,
    );
    fireEvent.click(screen.getByText('page'));
    fireEvent.click(screen.getByText('zoom'));

    expect(pages).toEqual([7]);
    expect(zoomed).toBe(1);
  });

  it('emits in order when several emitters fire in one handler', () => {
    const order: string[] = [];

    const Impl: RC<object, WidgetEvents> = ({ events }) => (
      <button
        type='button'
        onClick={() => {
          events.pageClicked({ index: 1 });
          events.zoomIn();
          events.renamed('last');
        }}
      >
        burst
      </button>
    );
    const Wrapped = withEvents(Impl);

    render(<Wrapped eventObserver={{ next: envelope => order.push(envelope.type) }} />);
    fireEvent.click(screen.getByText('burst'));

    expect(order).toEqual(['pageClicked', 'zoomIn', 'renamed']);
  });
});
