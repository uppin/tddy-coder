/**
 * A scope pairs building a resource with disposing of it.
 *
 * The alternative — construct at the top of `start`, reverse at the bottom — is a
 * checklist nothing checks: a sixth resource with no matching teardown line leaks
 * a poll loop for the life of the page, and the handler still compiles and the app
 * still works. So these tests are about disposal being a *consequence* of
 * construction, not about the factories returning the right shapes; the primitives
 * have their own tests for that.
 *
 * Every assertion is on observable behaviour — an adapter's call count, a store's
 * contents, a value that did or did not arrive — never on a `tornDown` flag.
 */
import { Subject, VirtualTimeScheduler } from 'rxjs';
import { describe, expect, it } from 'bun:test';

import { deferred } from '../testkit/index';

import { createStore } from '../stores/index';

import { createAppScope } from './index';

/** A virtual scheduler: the poll loop runs with no real time passing. */
const clock = (): VirtualTimeScheduler => new VirtualTimeScheduler();

/** Settles the microtask queue so a resolved call has been written to its store. */
const settle = (): Promise<void> => Promise.resolve().then(() => undefined);

describe('createAppScope — the resources it builds', () => {
  it('builds a request that reaches its adapter and publishes the answer', async () => {
    const scope = createAppScope();

    const request = scope.request(async (name: string) => `hello ${name}`);
    request.invoke('world');
    await settle();

    expect(request.state.getState()).toEqual({ status: 'ready', value: 'hello world' });
  });

  it('honours the options it is handed', async () => {
    // `queue` runs calls in order rather than abandoning the earlier one — the
    // scope forwards options rather than deciding them.
    const scope = createAppScope();
    const seen: number[] = [];

    const request = scope.request(
      async (value: number) => {
        seen.push(value);
        return value;
      },
      { concurrency: 'queue' },
    );
    request.invoke(1);
    request.invoke(2);
    await settle();
    await settle();

    expect(seen).toEqual([1, 2]);
  });

  it('builds a poller that does not poll until it is started', () => {
    // Nothing polls a server for a region nobody is looking at. The scope owns
    // disposal only; starting stays with the view.
    const scope = createAppScope();
    let calls = 0;

    scope.poller(
      async () => {
        calls += 1;
        return calls;
      },
      { intervalMs: 100, scheduler: clock() },
    );

    expect(calls).toBe(0);
  });

  it('builds a poller that polls once started', async () => {
    const scope = createAppScope();

    const poller = scope.poller(async () => 'jobs', { intervalMs: 100, scheduler: clock() });
    poller.start();
    await settle();

    expect(poller.latest.getState()).toEqual({ status: 'ready', value: 'jobs' });
  });

  it('builds an error channel that reports', () => {
    const scope = createAppScope();
    const failure = new Error('unreachable');
    const reported: unknown[] = [];

    const errors = scope.errorChannel();
    errors.errors$.subscribe(error => reported.push(error));
    errors.report('cancelJob', failure);

    expect(reported).toEqual([{ command: 'cancelJob', error: failure }]);
  });

  it('builds a subscription that delivers values', () => {
    const scope = createAppScope();
    const source = new Subject<string>();
    const seen: string[] = [];

    scope.subscribe(source, value => seen.push(value));
    source.next('first');
    source.next('second');

    expect(seen).toEqual(['first', 'second']);
  });
});

describe('createAppScope — closing it', () => {
  it('stops a running poller from calling its adapter again', async () => {
    const scope = createAppScope();
    const timer = clock();
    let calls = 0;
    const poller = scope.poller(
      async () => {
        calls += 1;
        return calls;
      },
      { intervalMs: 100, scheduler: timer },
    );
    poller.start();
    await settle();
    expect(calls).toBe(1);

    scope.close();
    poller.start();
    timer.flush();
    await settle();

    expect(calls).toBe(1);
  });

  it('keeps a poll that was in flight at close from ever landing', async () => {
    // Stopping the loop does not un-await the call already outstanding. This is the
    // line a hand-written teardown gets right only by accident.
    const scope = createAppScope();
    const gate = deferred<string>();
    const poller = scope.poller(() => gate.promise, { intervalMs: 100, scheduler: clock() });
    poller.start();
    await settle();

    scope.close();
    gate.resolve('late');
    await settle();

    expect(poller.latest.getState()).toEqual({ status: 'idle' });
  });

  it('suppresses a request that settles afterwards', async () => {
    const scope = createAppScope();
    const gate = deferred<string>();
    const request = scope.request(() => gate.promise);
    request.invoke();
    await settle();
    expect(request.state.getState()).toEqual({ status: 'loading' });

    scope.close();
    gate.resolve('late');
    await settle();

    expect(request.state.getState()).toEqual({ status: 'loading' });
  });

  it('suppresses a request that rejects afterwards', async () => {
    const scope = createAppScope();
    const gate = deferred<string>();
    const request = scope.request(() => gate.promise);
    request.invoke();
    await settle();

    scope.close();
    gate.reject(new Error('late failure'));
    await settle();

    expect(request.state.getState()).toEqual({ status: 'loading' });
  });

  it('stops the error channel from emitting', () => {
    const scope = createAppScope();
    const reported: unknown[] = [];
    const errors = scope.errorChannel();
    errors.errors$.subscribe(error => reported.push(error));

    scope.close();
    errors.report('cancelJob', new Error('after close'));

    expect(reported).toEqual([]);
  });

  it('unsubscribes a subscription it made', () => {
    const scope = createAppScope();
    const source = new Subject<string>();
    const seen: string[] = [];
    scope.subscribe(source, value => seen.push(value));
    source.next('before');

    scope.close();
    source.next('after');

    expect(seen).toEqual(['before']);
  });

  it('disposes in reverse registration order', () => {
    // Registration order is dependency order — an observer is registered after the
    // source it observes — so reversing it closes observers before their sources
    // and no disposal delivers a value to something already gone.
    const scope = createAppScope();
    const order: string[] = [];
    const first = scope.errorChannel();
    const second = scope.errorChannel();
    const third = scope.errorChannel();
    first.errors$.subscribe({ complete: () => order.push('first') });
    second.errors$.subscribe({ complete: () => order.push('second') });
    third.errors$.subscribe({ complete: () => order.push('third') });

    scope.close();

    expect(order).toEqual(['third', 'second', 'first']);
  });

  it('disposes every resource it holds, of every kind', async () => {
    const scope = createAppScope();
    const timer = clock();
    let polls = 0;
    const poller = scope.poller(
      async () => {
        polls += 1;
        return polls;
      },
      { intervalMs: 100, scheduler: timer },
    );
    const gate = deferred<string>();
    const request = scope.request(() => gate.promise);
    const errors = scope.errorChannel();
    const source = new Subject<string>();
    const seen: string[] = [];
    const reported: unknown[] = [];
    scope.subscribe(source, value => seen.push(value));
    errors.errors$.subscribe(error => reported.push(error));
    poller.start();
    request.invoke();
    await settle();

    scope.close();
    poller.start();
    timer.flush();
    gate.resolve('late');
    source.next('after');
    errors.report('send', new Error('after'));
    await settle();

    expect({
      polls,
      request: request.state.getState(),
      seen,
      reported,
    }).toEqual({
      polls: 1,
      request: { status: 'loading' },
      seen: [],
      reported: [],
    });
  });

  it('closing twice disposes each resource once and does not throw', () => {
    const scope = createAppScope();
    const order: string[] = [];
    const first = scope.errorChannel();
    const second = scope.errorChannel();
    first.errors$.subscribe({ complete: () => order.push('first') });
    second.errors$.subscribe({ complete: () => order.push('second') });

    scope.close();
    scope.close();

    expect(order).toEqual(['second', 'first']);
  });

  it('closes an empty scope without throwing', () => {
    const scope = createAppScope();

    expect(() => scope.close()).not.toThrow();
  });

  it('disposes a resource whose adapter has already failed', async () => {
    // A poll that errored is still a live loop; a scope that skipped it because it
    // was not in a ready state would leak exactly the case worth stopping.
    const scope = createAppScope();
    const timer = clock();
    let calls = 0;
    const poller = scope.poller(
      async () => {
        calls += 1;
        throw new Error('unreachable');
      },
      { intervalMs: 100, scheduler: timer },
    );
    poller.start();
    await settle();
    expect(calls).toBe(1);

    scope.close();
    poller.start();
    timer.flush();
    await settle();

    expect(calls).toBe(1);
  });

  it('never delivers a value to a subscription made after it has closed', async () => {
    // Disposing after the fact is not enough here: a `BehaviorSubject`-backed store
    // delivers its current value the moment you subscribe, so an observer that runs
    // once and is then unsubscribed has already written whatever it writes.
    const scope = createAppScope();
    const source = createStore<{ count: number }>(() => ({ count: 1 }));
    const seen: number[] = [];
    scope.close();

    const subscription = scope.subscribe(source, value => seen.push(value.count));
    source.setState({ count: 2 });
    await settle();

    expect({ seen, closed: subscription.closed }).toEqual({ seen: [], closed: true });
  });

  it('builds nothing new after it has closed', async () => {
    // A resource handed out by a closed scope has no owner, so it must arrive dead
    // rather than alive and unreachable.
    const scope = createAppScope();
    scope.close();

    let calls = 0;
    const poller = scope.poller(
      async () => {
        calls += 1;
        return calls;
      },
      { intervalMs: 100, scheduler: clock() },
    );
    poller.start();
    await settle();

    expect(calls).toBe(0);
  });
});

describe('createAppScope — what a handler may do with it', () => {
  /**
   * That `AppScope` omits `close` is checked by `tsc` compiling `withPorts`, not by
   * a test: this package excludes test files from its tsconfig, so an
   * `expectTypeOf` assertion here would never be evaluated. The runtime half of the
   * guarantee — that the object a handler receives does not carry `close` either —
   * is asserted in `ports.test.ts`, where a handler actually receives one.
   */
  it('returns the four factories a handler can call', () => {
    const scope = createAppScope();

    expect(
      (['request', 'poller', 'errorChannel', 'subscribe'] as const).map(name => typeof scope[name]),
    ).toEqual(['function', 'function', 'function', 'function']);
  });
});
