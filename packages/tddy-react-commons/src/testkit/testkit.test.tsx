import { fireEvent, render, screen } from '@testing-library/react';
import { type FC } from 'react';
import { BehaviorSubject } from 'rxjs';
import { describe, expect, it } from 'bun:test';

import { useAppContext } from '../container/index';
import { useEventObserver, withEvents, type RC } from '../events/index';
import type { Store } from '../stores/index';

import { AppContextStub, emittersFor, eventDriver, eventRecorder, stubStore } from './index';

interface DocState {
  pageCount: number;
  currentPage: number;
}

interface Commands {
  selectPage: (page: number) => void;
  reset: () => void;
}

interface TestContext {
  docStore: Store<DocState>;
  status$: BehaviorSubject<'loading' | 'ready'>;
  commands: Commands;
}

interface StripEvents {
  pageClicked: { index: number };
  cleared: null;
}

/** The component under test needs a real app context — in production that means a handler. */
const PageCount: FC = () => {
  const { docStore, commands } = useAppContext<TestContext>();
  const pageCount = docStore.useStore(state => state.pageCount);

  return (
    <>
      <span data-testid='count'>{pageCount}</span>
      <button type='button' onClick={() => commands.selectPage(4)}>
        pick
      </button>
    </>
  );
};

describe('AppContextStub', () => {
  it('renders a context-dependent component with no handler at all', () => {
    render(
      <AppContextStub<TestContext>
        value={{
          docStore: stubStore<DocState>({ pageCount: 9, currentPage: 1 }),
          status$: new BehaviorSubject<'loading' | 'ready'>('ready'),
          commands: { selectPage: () => {}, reset: () => {} },
        }}
      >
        <PageCount />
      </AppContextStub>,
    );

    expect(screen.getByTestId('count').textContent).toBe('9');
  });
});

describe('stubStore', () => {
  it('behaves as a real store', () => {
    const store = stubStore<DocState>({ pageCount: 3, currentPage: 1 });

    store.setState({ currentPage: 2 });

    expect(store.getState()).toEqual({ pageCount: 3, currentPage: 2 });
  });

  it('records patches passed to setState, including updater results', () => {
    const store = stubStore<DocState>({ pageCount: 3, currentPage: 1 });

    store.setState({ currentPage: 2 });
    store.setState(state => ({ currentPage: state.currentPage + 1 }));

    expect(store.writes).toEqual([{ currentPage: 2 }, { currentPage: 3 }]);
  });

  it('records whole-state replacements passed to next', () => {
    const store = stubStore<DocState>({ pageCount: 3, currentPage: 1 });

    store.next({ pageCount: 5, currentPage: 5 });

    expect(store.replacements).toEqual([{ pageCount: 5, currentPage: 5 }]);
    expect(store.getState()).toEqual({ pageCount: 5, currentPage: 5 });
  });

  it('clears its log on resetWrites', () => {
    const store = stubStore<DocState>({ pageCount: 1, currentPage: 1 });
    store.setState({ currentPage: 2 });

    store.resetWrites();

    expect(store.writes).toEqual([]);
  });
});

describe('eventRecorder', () => {
  it('captures what a component emits, typed per event name', () => {
    const StripImpl: RC<object, StripEvents> = ({ events }) => (
      <>
        <button type='button' onClick={() => events.pageClicked({ index: 3 })}>
          page
        </button>
        <button type='button' onClick={() => events.cleared()}>
          clear
        </button>
      </>
    );
    const Strip = withEvents(StripImpl);
    const events = eventRecorder<StripEvents>();

    render(<Strip eventObserver={events.observer} />);
    fireEvent.click(screen.getByText('page'));
    fireEvent.click(screen.getByText('clear'));

    expect(events.types).toEqual(['pageClicked', 'cleared']);
    expect(events.lastOf('pageClicked')).toEqual({ index: 3 });
    expect(events.payloadsOf('pageClicked')).toEqual([{ index: 3 }]);
    expect(events.countOf('cleared')).toBe(1);
    expect(events.lastOf('cleared')).toBeNull();
  });

  it('resets its log', () => {
    const events = eventRecorder<StripEvents>();
    events.observer.next?.({ type: 'cleared', payload: null });

    events.reset();

    expect(events.emitted).toEqual([]);
  });
});

describe('emittersFor', () => {
  it('drives a parent handler map without rendering the real child', () => {
    const pages: number[] = [];
    let cleared = 0;
    let observer: ReturnType<typeof useEventObserver<StripEvents>> | undefined;

    const Parent: FC = () => {
      observer = useEventObserver<StripEvents>({
        pageClicked: ({ index }) => pages.push(index),
        cleared: () => {
          cleared += 1;
        },
      });
      return null;
    };

    render(<Parent />);

    // Same typed surface the child would have received.
    const events = emittersFor<StripEvents>(observer!);
    events.pageClicked({ index: 7 });
    events.cleared();

    expect(pages).toEqual([7]);
    expect(cleared).toBe(1);
  });

  it('normalises a no-argument call to null, matching a mounted child', () => {
    const events = eventRecorder<StripEvents>();

    emittersFor<StripEvents>(events.observer).cleared();

    expect(events.emitted).toEqual([{ type: 'cleared', payload: null }]);
  });

  it('still accepts a full envelope via next', () => {
    const events = eventRecorder<StripEvents>();

    emittersFor<StripEvents>(events.observer).next({ type: 'pageClicked', payload: { index: 1 } });

    expect(events.lastOf('pageClicked')).toEqual({ index: 1 });
  });
});

describe('eventDriver', () => {
  it('emits and records with no component involved', () => {
    const { events, recorder } = eventDriver<StripEvents>();

    events.pageClicked({ index: 2 });
    events.cleared();

    expect(recorder.types).toEqual(['pageClicked', 'cleared']);
    expect(recorder.lastOf('pageClicked')).toEqual({ index: 2 });
  });

  it('forwards to a target observer so a sink can be exercised directly', () => {
    const store = stubStore<DocState>({ pageCount: 0, currentPage: 1 });
    const sinkObserver = {
      next: (envelope: { type: keyof StripEvents; payload: unknown }) => {
        if (envelope.type === 'pageClicked') {
          store.setState({ currentPage: (envelope.payload as { index: number }).index });
        }
      },
    };

    const { events, recorder } = eventDriver<StripEvents>(sinkObserver);
    events.pageClicked({ index: 5 });

    expect(store.writes).toEqual([{ currentPage: 5 }]);
    expect(recorder.countOf('pageClicked')).toBe(1);
  });
});
