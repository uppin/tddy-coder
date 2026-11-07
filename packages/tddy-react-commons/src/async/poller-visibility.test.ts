import { VirtualTimeScheduler } from 'rxjs';
import { afterEach, describe, expect, it } from 'bun:test';

import { createPoller } from './poller';

const settle = (): Promise<void> => Promise.resolve().then(() => undefined);

let visibility: DocumentVisibilityState = 'visible';

function setVisibility(state: DocumentVisibilityState): void {
  visibility = state;
  document.dispatchEvent(new Event('visibilitychange'));
}

Object.defineProperty(document, 'visibilityState', {
  configurable: true,
  get: () => visibility,
});

afterEach(() => {
  visibility = 'visible';
});

describe('createPoller on a hidden tab', () => {
  it('pauses while the document is hidden and polls again when it is visible', async () => {
    const clock = new VirtualTimeScheduler();
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
    expect(calls).toBe(1);

    setVisibility('hidden');
    clock.flush();
    await settle();
    expect(calls).toBe(1);

    setVisibility('visible');
    await settle();
    expect(calls).toBe(2);

    poller.teardown();
  });
});
