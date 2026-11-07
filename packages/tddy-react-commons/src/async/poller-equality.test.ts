import { VirtualTimeScheduler } from 'rxjs';
import { afterEach, describe, expect, it } from 'bun:test';

import { createPoller, type PollOptions } from './poller';
import type { RequestState } from './index';

const settle = (): Promise<void> => Promise.resolve().then(() => undefined);
const disposers: Array<() => void> = [];

afterEach(() => disposers.splice(0).forEach(dispose => dispose()));

function polling<T>(initial: T, options: Partial<PollOptions<T>> = {}) {
  let answer = initial;
  let failure: Error | undefined;
  const clock = new VirtualTimeScheduler();
  const emissions: RequestState<T>[] = [];
  const poller = createPoller(
    async () => {
      if (failure) {
        throw failure;
      }
      return answer;
    },
    { intervalMs: 500, scheduler: clock, ...options },
  );
  const subscription = poller.latest.subscribe(state => emissions.push(state));
  const close = (): void => {
    subscription.unsubscribe();
    poller.teardown();
  };
  disposers.push(close);

  return {
    async started() {
      poller.start();
      await settle();
      emissions.length = 0;
      return this;
    },
    async receiving(next: T) {
      answer = next;
      failure = undefined;
      poller.refresh();
      await settle();
    },
    async failing() {
      failure = new Error('jobs unavailable');
      poller.refresh();
      await settle();
    },
    snapshot: () => poller.latest.getState(),
    emissions,
    close,
  };
}

describe('unchanged poll results', () => {
  it('uses the supplied equality when a caller compares only displayed progress', async () => {
    const training = await polling(
      { step: 12, checkedAt: 1 },
      { equality: (previous, next) => previous.step === next.step },
    ).started();

    await training.receiving({ step: 12, checkedAt: 2 });

    expect(training.emissions).toEqual([]);
    training.close();
  });

  it('emits nothing when a fresh response contains the same nested data', async () => {
    const training = await polling([{ id: 'train-1', progress: { step: 12 } }]).started();
    const first = training.snapshot();

    await training.receiving([{ id: 'train-1', progress: { step: 12 } }]);

    expect(training.emissions).toEqual([]);
    expect(training.snapshot()).toBe(first);
    training.close();
  });

  it('publishes a changed nested progress value', async () => {
    const training = await polling([{ id: 'train-1', progress: { step: 12 } }]).started();

    await training.receiving([{ id: 'train-1', progress: { step: 13 } }]);

    expect(training.emissions).toEqual([
      { status: 'ready', value: [{ id: 'train-1', progress: { step: 13 } }] },
    ]);
    training.close();
  });

  it('publishes recovery even when the answer equals the last successful response', async () => {
    const training = await polling({ step: 12 }).started();
    await training.failing();
    training.emissions.length = 0;

    await training.receiving({ step: 12 });

    expect(training.emissions).toEqual([{ status: 'ready', value: { step: 12 } }]);
    training.close();
  });

  it('publishes the first successful undefined answer', async () => {
    const training = await polling(undefined).started();

    const snapshot = training.snapshot();

    expect(snapshot).toEqual({ status: 'ready', value: undefined });
    training.close();
  });
});
