import { describe, expect, it } from 'bun:test';

import { deferred, stubPorts } from './index';

interface ChatPort {
  send(text: string): Promise<string>;
}

interface JobPort {
  list(): Promise<string[]>;
}

interface TestPorts {
  chat: ChatPort;
  jobs: JobPort;
  [key: string]: unknown;
}

describe('stubPorts', () => {
  it('exposes what the test defined', async () => {
    const ports = stubPorts<TestPorts>({ chat: { send: async () => 'answer' } });

    await expect(ports.chat.send('hi')).resolves.toBe('answer');
  });

  it('throws naming the port a test forgot to stub', () => {
    // `undefined` would surface two frames later as "cannot read property of
    // undefined", pointing at the consumer rather than the double.
    const ports = stubPorts<TestPorts>({ chat: { send: async () => 'answer' } });

    expect(() => ports.jobs).toThrow(/jobs/);
  });
});

describe('deferred', () => {
  it('stays pending until resolved', async () => {
    const pending = deferred<string>();
    let settled = false;
    void pending.promise.then(() => {
      settled = true;
    });

    await Promise.resolve();

    expect(settled).toBe(false);
  });

  it('resolves with the value handed to resolve', async () => {
    const pending = deferred<string>();

    pending.resolve('answer');

    await expect(pending.promise).resolves.toBe('answer');
  });

  it('rejects with the error handed to reject', async () => {
    const pending = deferred<string>();
    const failure = new Error('nope');

    pending.reject(failure);

    await expect(pending.promise).rejects.toBe(failure);
  });
});
