import { VirtualTimeScheduler } from 'rxjs';
import { describe, expect, it } from 'bun:test';

import { createPoller } from './poller';

/**
 * A virtual scheduler rather than fake timers or a sleep: the poll loop is
 * exercised with no real time passing and no wall-clock flake, and `flush()` is an
 * exact statement of how far the loop was allowed to run.
 */
const scheduler = (): VirtualTimeScheduler => new VirtualTimeScheduler();

/** Settles the microtask queue so a resolved poll has been written to the store. */
const settle = (): Promise<void> => Promise.resolve().then(() => undefined);

describe('createPoller', () => {
  it('does not call anything until started', () => {
    let calls = 0;
    createPoller(
      async () => {
        calls += 1;
        return calls;
      },
      { intervalMs: 100, scheduler: scheduler() },
    );

    expect(calls).toBe(0);
  });

  it('reports not running before start', () => {
    const poller = createPoller(async () => 1, { intervalMs: 100, scheduler: scheduler() });

    expect(poller.running.getState()).toBe(false);
  });

  it('polls immediately on start rather than waiting out the first interval', async () => {
    // A jobs panel that shows nothing for the first interval reads as broken.
    let calls = 0;
    const poller = createPoller(
      async () => {
        calls += 1;
        return calls;
      },
      { intervalMs: 100, scheduler: scheduler() },
    );

    poller.start();
    await settle();

    expect(calls).toBe(1);
  });

  it('repeats on the interval', async () => {
    const clock = scheduler();
    let calls = 0;
    const poller = createPoller(
      async () => {
        calls += 1;
        return calls;
      },
      { intervalMs: 100, scheduler: clock },
    );

    poller.start();
    await settle();
    clock.flush();
    await settle();

    expect(calls).toBeGreaterThan(1);
  });

  it('publishes the latest answer', async () => {
    const poller = createPoller(async () => 'jobs', { intervalMs: 100, scheduler: scheduler() });

    poller.start();
    await settle();

    expect(poller.latest.getState()).toEqual({ status: 'ready', value: 'jobs' });
  });

  it('stop halts the loop', async () => {
    const clock = scheduler();
    let calls = 0;
    const poller = createPoller(
      async () => {
        calls += 1;
        return calls;
      },
      { intervalMs: 100, scheduler: clock },
    );

    poller.start();
    await settle();
    poller.stop();
    const atStop = calls;
    clock.flush();
    await settle();

    expect(calls).toBe(atStop);
  });

  it('keeps the last answer after stopping', async () => {
    // A stopped poller is not a cleared one; the view usually keeps showing it.
    const poller = createPoller(async () => 'jobs', { intervalMs: 100, scheduler: scheduler() });
    poller.start();
    await settle();

    poller.stop();

    expect(poller.latest.getState()).toEqual({ status: 'ready', value: 'jobs' });
  });

  it('a failed poll does not end the loop', async () => {
    const clock = scheduler();
    let calls = 0;
    const poller = createPoller(
      async () => {
        calls += 1;
        if (calls === 1) {
          throw new Error('transient');
        }
        return calls;
      },
      { intervalMs: 100, scheduler: clock },
    );

    poller.start();
    await settle();
    clock.flush();
    await settle();

    expect(calls).toBeGreaterThan(1);
  });

  it('publishes the failure rather than swallowing it', async () => {
    const failure = new Error('unreachable');
    const poller = createPoller(
      async () => {
        throw failure;
      },
      { intervalMs: 100, scheduler: scheduler() },
    );

    poller.start();
    await settle();

    expect(poller.latest.getState()).toEqual({ status: 'error', error: failure });
  });

  it('consults backoffMs after a failure, with the consecutive count', async () => {
    const clock = scheduler();
    const asked: number[] = [];
    const poller = createPoller(
      async () => {
        throw new Error('down');
      },
      {
        intervalMs: 100,
        scheduler: clock,
        backoffMs: failures => {
          asked.push(failures);
          return failures * 100;
        },
      },
    );

    poller.start();
    await settle();
    clock.flush();
    await settle();

    expect(asked[0]).toBe(1);
  });

  it('start is idempotent — a second call does not run two loops', async () => {
    let calls = 0;
    const poller = createPoller(
      async () => {
        calls += 1;
        return calls;
      },
      { intervalMs: 100, scheduler: scheduler() },
    );

    poller.start();
    poller.start();
    await settle();

    expect(calls).toBe(1);
  });

  it('teardown stops the loop and further starts do nothing', async () => {
    const clock = scheduler();
    let calls = 0;
    const poller = createPoller(
      async () => {
        calls += 1;
        return calls;
      },
      { intervalMs: 100, scheduler: clock },
    );

    poller.teardown();
    poller.start();
    clock.flush();
    await settle();

    expect(calls).toBe(0);
  });
});
