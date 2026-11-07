import { VirtualTimeScheduler } from 'rxjs';
import { describe, expect, it, vi } from 'bun:test';

import { deferred } from '../testkit/index';
import type { VisibilitySource } from './poller';
import { createPoller } from './poller';

const settle = (): Promise<void> => Promise.resolve().then(() => undefined);

/** Lets an awaited poll publish and schedule its follow-up, which takes several microtasks. */
async function settleChain(): Promise<void> {
  await settle();
  await settle();
  await settle();
  await settle();
}

function manualVisibility() {
  let visible = true;
  const listeners = new Set<() => void>();
  const source: VisibilitySource = {
    isVisible: () => visible,
    subscribe: onChange => {
      listeners.add(onChange);
      return () => {
        listeners.delete(onChange);
      };
    },
  };
  return {
    source,
    set(next: boolean): void {
      visible = next;
      listeners.forEach(listener => listener());
    },
    listenerCount: (): number => listeners.size,
  };
}

function countingPoller(clock: VirtualTimeScheduler, visibility = manualVisibility()) {
  const counter = { calls: 0 };
  const poller = createPoller(
    async () => {
      counter.calls += 1;
      return counter.calls;
    },
    { intervalMs: 100, scheduler: clock, visibility: visibility.source },
  );
  return { counter, poller, visibility };
}

describe('Poller.refresh', () => {
  it('polls now rather than waiting out the interval', async () => {
    const { counter, poller } = countingPoller(new VirtualTimeScheduler());
    poller.start();
    await settleChain();

    poller.refresh();
    await settleChain();

    expect(counter.calls).toBe(2);
  });

  it('polls once in the next interval after a refresh, not twice', async () => {
    const clock = new VirtualTimeScheduler();
    const { counter, poller } = countingPoller(clock);
    poller.start();
    await settleChain();
    poller.refresh();
    await settleChain();

    clock.flush();
    await settleChain();

    expect(counter.calls).toBe(3);
  });

  it('polls once more after an in-flight poll, however many refreshes arrived during it', async () => {
    const answers = [deferred<string>(), deferred<string>()];
    let calls = 0;
    const poller = createPoller(
      () => {
        calls += 1;
        return answers[calls - 1].promise;
      },
      {
        intervalMs: 100,
        scheduler: new VirtualTimeScheduler(),
        visibility: manualVisibility().source,
      },
    );
    poller.start();

    poller.refresh();
    poller.refresh();
    poller.refresh();
    expect(calls).toBe(1);

    answers[0].resolve('stale');
    await settleChain();
    expect(calls).toBe(2);

    answers[1].resolve('fresh');
    await settleChain();
    expect(calls).toBe(2);
    expect(poller.latest.getState()).toEqual({ status: 'ready', value: 'fresh' });
  });

  it('does nothing before the poller starts', async () => {
    const { counter, poller } = countingPoller(new VirtualTimeScheduler());

    poller.refresh();
    await settleChain();

    expect(counter.calls).toBe(0);
  });

  it('does nothing after the poller stops', async () => {
    const { counter, poller } = countingPoller(new VirtualTimeScheduler());
    poller.start();
    await settleChain();
    poller.stop();

    poller.refresh();
    await settleChain();

    expect(counter.calls).toBe(1);
  });

  it('does nothing while the page is hidden', async () => {
    const { counter, poller, visibility } = countingPoller(new VirtualTimeScheduler());
    poller.start();
    await settleChain();
    visibility.set(false);

    poller.refresh();
    await settleChain();

    expect(counter.calls).toBe(1);
  });

  it('does nothing after teardown', async () => {
    const { counter, poller } = countingPoller(new VirtualTimeScheduler());
    poller.start();
    await settleChain();
    poller.teardown();

    poller.refresh();
    await settleChain();

    expect(counter.calls).toBe(1);
  });
});

describe('PollOptions.nextIntervalMs', () => {
  it('asks for the next delay with the answer the poll returned', async () => {
    const nextIntervalMs = vi.fn((_value: string) => 250);
    const poller = createPoller(async () => 'jobs', {
      intervalMs: 100,
      nextIntervalMs,
      scheduler: new VirtualTimeScheduler(),
      visibility: manualVisibility().source,
    });

    poller.start();
    await settleChain();

    expect(nextIntervalMs).toHaveBeenCalledWith('jobs');
  });

  it('waits the delay it returns before the next poll', async () => {
    const clock = new VirtualTimeScheduler();
    const poller = createPoller(async () => 'jobs', {
      intervalMs: 100,
      nextIntervalMs: () => 250,
      scheduler: clock,
      visibility: manualVisibility().source,
    });
    poller.start();
    await settleChain();

    clock.flush();

    expect(clock.frame).toBe(250);
  });

  it('leaves the delay after a failure to backoffMs', async () => {
    const clock = new VirtualTimeScheduler();
    const nextIntervalMs = vi.fn(() => 250);
    const poller = createPoller<string>(
      async () => {
        throw new Error('down');
      },
      {
        intervalMs: 100,
        nextIntervalMs,
        backoffMs: () => 700,
        scheduler: clock,
        visibility: manualVisibility().source,
      },
    );
    poller.start();
    await settleChain();

    clock.flush();

    expect(nextIntervalMs).not.toHaveBeenCalled();
    expect(clock.frame).toBe(700);
  });

  it('publishes only the error when choosing the next delay throws', async () => {
    const poller = createPoller(async () => 'jobs', {
      intervalMs: 100,
      nextIntervalMs: () => {
        throw new Error('no delay');
      },
      scheduler: new VirtualTimeScheduler(),
      visibility: manualVisibility().source,
    });
    const statuses: string[] = [];
    poller.latest.subscribe(state => statuses.push(state.status));

    poller.start();
    await settleChain();

    expect(statuses).toEqual(['idle', 'error']);
  });
});

describe('PollOptions.visibility', () => {
  it('drops the pending tick when the page is hidden', async () => {
    const clock = new VirtualTimeScheduler();
    const { counter, poller, visibility } = countingPoller(clock);
    poller.start();
    await settleChain();

    visibility.set(false);
    clock.flush();
    await settleChain();

    expect(counter.calls).toBe(1);
  });

  it('polls now when the page is visible again', async () => {
    const { counter, poller, visibility } = countingPoller(new VirtualTimeScheduler());
    poller.start();
    await settleChain();
    visibility.set(false);

    visibility.set(true);
    await settleChain();

    expect(counter.calls).toBe(2);
  });

  it('stays quiet when the page is visible again but the poller is stopped', async () => {
    const { counter, poller, visibility } = countingPoller(new VirtualTimeScheduler());
    poller.start();
    await settleChain();
    poller.stop();
    visibility.set(false);

    visibility.set(true);
    await settleChain();

    expect(counter.calls).toBe(1);
  });

  it('publishes a poll that was in flight when the page hid, but schedules no next poll', async () => {
    const clock = new VirtualTimeScheduler();
    const answer = deferred<string>();
    let calls = 0;
    const visibility = manualVisibility();
    const poller = createPoller(
      () => {
        calls += 1;
        return answer.promise;
      },
      { intervalMs: 100, scheduler: clock, visibility: visibility.source },
    );
    poller.start();
    visibility.set(false);

    answer.resolve('jobs');
    await settleChain();
    clock.flush();
    await settleChain();

    expect(poller.latest.getState()).toEqual({ status: 'ready', value: 'jobs' });
    expect(calls).toBe(1);
  });

  it('removes its visibility listener on teardown', async () => {
    const { poller, visibility } = countingPoller(new VirtualTimeScheduler());
    poller.start();
    await settleChain();
    expect(visibility.listenerCount()).toBe(1);

    poller.teardown();

    expect(visibility.listenerCount()).toBe(0);
  });
});

describe('Poller.start', () => {
  it('polls again when a poll from before the last stop is still in flight', async () => {
    const answers = [deferred<string>(), deferred<string>()];
    let calls = 0;
    const poller = createPoller(
      () => {
        calls += 1;
        return answers[calls - 1].promise;
      },
      {
        intervalMs: 100,
        scheduler: new VirtualTimeScheduler(),
        visibility: manualVisibility().source,
      },
    );
    poller.start();
    poller.stop();
    poller.start();

    answers[0].resolve('stale');
    await settleChain();
    expect(calls).toBe(2);

    answers[1].resolve('fresh');
    await settleChain();
    expect(poller.latest.getState()).toEqual({ status: 'ready', value: 'fresh' });
  });
});
