import { describe, expect, it } from 'bun:test';

import { createCommandErrorChannel, type CommandError } from './index';

const collect = (): {
  seen: CommandError[];
  channel: ReturnType<typeof createCommandErrorChannel>;
} => {
  const channel = createCommandErrorChannel();
  const seen: CommandError[] = [];
  channel.errors$.subscribe(error => seen.push(error));
  return { seen, channel };
};

describe('createCommandErrorChannel', () => {
  it('delivers a reported failure to subscribers', () => {
    const { seen, channel } = collect();
    const failure = new Error('boom');

    channel.report('sendMessage', failure);

    expect(seen).toEqual([{ command: 'sendMessage', error: failure }]);
  });

  it('names the command the view asked for, not the adapter method', () => {
    const { seen, channel } = collect();

    channel.report('sendMessage', new Error('CreateChatCompletion failed'));

    expect(seen[0]?.command).toBe('sendMessage');
  });

  it('delivers nothing that happened before subscribing', () => {
    // A Subject, not a BehaviorSubject: an error is an event, and replaying the
    // last one to a component that mounts later shows a stale failure.
    const channel = createCommandErrorChannel();
    channel.report('early', new Error('missed'));

    const seen: CommandError[] = [];
    channel.errors$.subscribe(error => seen.push(error));

    expect(seen).toEqual([]);
  });

  it('reports every failure, including repeats of the same command', () => {
    const { seen, channel } = collect();

    channel.report('retry', new Error('first'));
    channel.report('retry', new Error('second'));

    expect(seen).toHaveLength(2);
  });

  it('carries a non-Error rejection unchanged', () => {
    const { seen, channel } = collect();

    channel.report('sendMessage', 'a string rejection');

    expect(seen[0]?.error).toBe('a string rejection');
  });

  describe('guard', () => {
    it('reports true when the body succeeds, and says nothing', async () => {
      const { seen, channel } = collect();

      const ok = await channel.guard('sendMessage', async () => undefined);

      expect(ok).toBe(true);
      expect(seen).toEqual([]);
    });

    it('reports false and publishes when the body rejects', async () => {
      const { seen, channel } = collect();
      const failure = new Error('nope');

      const ok = await channel.guard('sendMessage', async () => {
        throw failure;
      });

      expect(ok).toBe(false);
      expect(seen).toEqual([{ command: 'sendMessage', error: failure }]);
    });

    it('does not rethrow, so a handler pipeline is not torn down by one failure', async () => {
      const { channel } = collect();

      await expect(
        channel.guard('sendMessage', async () => {
          throw new Error('nope');
        }),
      ).resolves.toBe(false);
    });
  });

  it('teardown stops delivery', () => {
    const { seen, channel } = collect();

    channel.teardown();
    channel.report('afterwards', new Error('ignored'));

    expect(seen).toEqual([]);
  });
});
