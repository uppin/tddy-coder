import type { JSX as ReactJSX } from 'react';
import { act, cleanup, render, screen } from '@testing-library/react';
import { Profiler } from 'react';
import { afterEach, describe, expect, it, vi } from 'bun:test';

import { createPoller, type Poller } from './poller';

interface Progress {
  readonly step: number;
}

function TrainingProgress({ poller }: { readonly poller: Poller<Progress> }): ReactJSX.Element {
  const state = poller.latest.useStore();
  return <span>{state.status === 'ready' ? `Step ${state.value.step}` : state.status}</span>;
}

async function passTenSeconds(): Promise<void> {
  for (let tick = 0; tick < 20; tick += 1) {
    await act(async () => {
      vi.advanceTimersByTime(500);
      await Promise.resolve();
    });
  }
}

afterEach(() => {
  cleanup();
  vi.useRealTimers();
});

describe('rendering unchanged polling data', () => {
  it('keeps the visible progress without committing during ten seconds of unchanged ticks', async () => {
    vi.useFakeTimers();
    let calls = 0;
    const poller = createPoller(
      async () => {
        calls += 1;
        return { step: 12 };
      },
      { intervalMs: 500 },
    );
    const commits: string[] = [];
    render(
      <Profiler id='training-progress' onRender={(_, phase) => commits.push(phase)}>
        <TrainingProgress poller={poller} />
      </Profiler>,
    );
    await act(async () => poller.start());
    commits.length = 0;

    await passTenSeconds();

    expect(calls).toBe(21);
    expect(screen.getByText('Step 12').isConnected).toBe(true);
    expect(commits).toHaveLength(0);
    poller.teardown();
  });
});
