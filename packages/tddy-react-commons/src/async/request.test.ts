import { describe, expect, it } from 'bun:test';

import { deferred } from '../testkit/index';

import { createRequest, isPending, isReady } from './index';

describe('createRequest', () => {
  it('starts idle, having called nothing', () => {
    let calls = 0;
    const request = createRequest(async () => {
      calls += 1;
      return 'value';
    });

    expect(request.state.getState()).toEqual({ status: 'idle' });
    expect(calls).toBe(0);
  });

  it('reports loading while the call is outstanding', () => {
    const pending = deferred<string>();
    const request = createRequest(() => pending.promise);

    request.invoke();

    expect(request.state.getState()).toEqual({ status: 'loading' });
  });

  it('carries the resolved value', async () => {
    const pending = deferred<string>();
    const request = createRequest(() => pending.promise);
    request.invoke();

    pending.resolve('answer');
    await pending.promise;

    expect(request.state.getState()).toEqual({ status: 'ready', value: 'answer' });
  });

  it('passes arguments through to the call', () => {
    const seen: unknown[][] = [];
    const request = createRequest(async (a: string, b: number) => {
      seen.push([a, b]);
      return a;
    });

    request.invoke('x', 2);

    expect(seen).toEqual([['x', 2]]);
  });

  it('turns a rejection into an error state rather than an unhandled rejection', async () => {
    const pending = deferred<string>();
    const failure = new Error('nope');
    const request = createRequest(() => pending.promise);
    request.invoke();

    pending.reject(failure);
    await pending.promise.catch(() => undefined);

    expect(request.state.getState()).toEqual({ status: 'error', error: failure });
  });

  it('stays usable after a failure', async () => {
    // An rxjs error would terminate the subscription, leaving a dead Request that
    // silently ignores every later invoke. This is the test that pins that.
    const first = deferred<string>();
    const second = deferred<string>();
    const calls = [first, second];
    let index = 0;
    const request = createRequest(() => calls[index++]!.promise);

    request.invoke();
    first.reject(new Error('first failed'));
    await first.promise.catch(() => undefined);

    request.invoke();
    second.resolve('recovered');
    await second.promise;

    expect(request.state.getState()).toEqual({ status: 'ready', value: 'recovered' });
  });

  it('reset returns to idle', async () => {
    const pending = deferred<string>();
    const request = createRequest(() => pending.promise);
    request.invoke();
    pending.resolve('answer');
    await pending.promise;

    request.reset();

    expect(request.state.getState()).toEqual({ status: 'idle' });
  });

  describe('concurrency', () => {
    it('abandons an in-flight call by default', async () => {
      const stale = deferred<string>();
      const fresh = deferred<string>();
      const calls = [stale, fresh];
      let index = 0;
      const request = createRequest(() => calls[index++]!.promise);

      request.invoke();
      request.invoke();
      fresh.resolve('newest');
      await fresh.promise;
      stale.resolve('stale');
      await stale.promise;

      expect(request.state.getState()).toEqual({ status: 'ready', value: 'newest' });
    });

    it('queued calls settle in order', async () => {
      const first = deferred<string>();
      const second = deferred<string>();
      const calls = [first, second];
      let index = 0;
      const request = createRequest(() => calls[index++]!.promise, { concurrency: 'queue' });

      request.invoke();
      request.invoke();
      first.resolve('first');
      await first.promise;
      second.resolve('second');
      await second.promise;

      expect(request.state.getState()).toEqual({ status: 'ready', value: 'second' });
    });

    it('a queued call does not start until the one before it settles', () => {
      const first = deferred<string>();
      let started = 0;
      const request = createRequest(
        () => {
          started += 1;
          return first.promise;
        },
        { concurrency: 'queue' },
      );

      request.invoke();
      request.invoke();

      expect(started).toBe(1);
    });
  });

  it('teardown stops a settling call from writing state', async () => {
    const pending = deferred<string>();
    const request = createRequest(() => pending.promise);
    request.invoke();

    request.teardown();
    pending.resolve('too late');
    await pending.promise;

    expect(request.state.getState()).toEqual({ status: 'loading' });
  });
});

describe('state predicates', () => {
  it('isReady narrows to the state carrying a value', () => {
    expect(isReady({ status: 'ready', value: 1 })).toBe(true);
    expect(isReady({ status: 'idle' })).toBe(false);
    expect(isReady({ status: 'loading' })).toBe(false);
    expect(isReady({ status: 'error', error: new Error('x') })).toBe(false);
  });

  it('isPending is true only while a call is outstanding', () => {
    expect(isPending({ status: 'loading' })).toBe(true);
    expect(isPending({ status: 'idle' })).toBe(false);
    expect(isPending({ status: 'ready', value: 1 })).toBe(false);
    expect(isPending({ status: 'error', error: new Error('x') })).toBe(false);
  });
});
