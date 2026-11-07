import { VirtualTimeScheduler } from 'rxjs';
import { describe, expect, it } from 'bun:test';

import type { Poller } from '../async/poller';
import type { AppHandle } from '../container/index';

import { withPorts, type AppScope, type PortedAppHandle, type PortedAppHandler } from './index';

interface ChatPort {
  send(text: string): Promise<string>;
}

interface TestPorts {
  chat: ChatPort;
  [key: string]: unknown;
}

interface TestContext {
  greeting: string;
  ports: TestPorts;
}

class RecordingHandler implements PortedAppHandler<TestPorts, TestContext> {
  starts = 0;

  start(ports: TestPorts): PortedAppHandle<TestContext> {
    this.starts += 1;
    return { context: { greeting: 'hello', ports } };
  }
}

const portsFixture = (): TestPorts => ({ chat: { send: async () => 'answer' } });

describe('withPorts', () => {
  it('produces a handler the container can start with no arguments', () => {
    const handler = withPorts(new RecordingHandler(), portsFixture());

    const handle = handler.start();

    expect(handle.context.greeting).toBe('hello');
  });

  it('hands the bound ports to the underlying handler', () => {
    const ports = portsFixture();

    const handle = withPorts(new RecordingHandler(), ports).start();

    expect(handle.context.ports).toBe(ports);
  });

  it('is referentially stable for the same handler and ports', () => {
    // The container restarts on handler-identity change, so a fresh object per
    // render would tear the app down and start it again on every commit.
    const inner = new RecordingHandler();
    const ports = portsFixture();

    expect(withPorts(inner, ports)).toBe(withPorts(inner, ports));
  });

  it('produces a different handler when the ports change', () => {
    const inner = new RecordingHandler();

    expect(withPorts(inner, portsFixture())).not.toBe(withPorts(inner, portsFixture()));
  });

  it('does not start the handler until the container asks', () => {
    const inner = new RecordingHandler();

    withPorts(inner, portsFixture());

    expect(inner.starts).toBe(0);
  });
});

/**
 * The scope seam.
 *
 * A ported handler is *handed* the thing that owns its long-lived resources, the
 * same way it is handed its adapters. What `withPorts` owes it: a scope per start
 * that the handler cannot close itself, and an `AppHandle` the container can tear
 * down without knowing a scope exists.
 */
interface ScopedContext {
  ports: TestPorts;
  poll: Poller<number>;
}

class ScopedHandler implements PortedAppHandler<TestPorts, ScopedContext> {
  readonly scopes: AppScope[] = [];

  readonly clock = new VirtualTimeScheduler();

  calls = 0;

  start(ports: TestPorts, scope: AppScope): PortedAppHandle<ScopedContext> {
    this.scopes.push(scope);

    const poll = scope.poller(
      async () => {
        this.calls += 1;
        return this.calls;
      },
      { intervalMs: 100, scheduler: this.clock },
    );

    return { context: { ports, poll } };
  }
}

/** Settles the microtask queue so a resolved poll has been written to its store. */
const settle = (): Promise<void> => Promise.resolve().then(() => undefined);

describe('withPorts and the app scope', () => {
  it('hands the handler a scope it can build resources with', async () => {
    const inner = new ScopedHandler();

    const handle = withPorts(inner, portsFixture()).start();
    handle.context.poll.start();
    await settle();

    expect(inner.calls).toBe(1);
  });

  it('gives the handler a scope it cannot close', () => {
    // A handler able to end its own app would leave the container holding a dead
    // handle. `close` belongs to the creator only.
    const inner = new ScopedHandler();

    withPorts(inner, portsFixture()).start();

    expect('close' in inner.scopes[0]).toBe(false);
  });

  it('creates a fresh scope for each start', () => {
    // The AppHandler object is memoised per (handler, ports) and outlives a mount,
    // so a scope built beside it would be reused after the first unmount closed it.
    const inner = new ScopedHandler();
    const handler = withPorts(inner, portsFixture());

    handler.start().teardown();
    handler.start();

    expect(inner.scopes[0]).not.toBe(inner.scopes[1]);
  });

  it('tears down the scope the handler was given', async () => {
    const inner = new ScopedHandler();
    const handle = withPorts(inner, portsFixture()).start();
    handle.context.poll.start();
    await settle();

    handle.teardown();
    handle.context.poll.start();
    inner.clock.flush();
    await settle();

    expect(inner.calls).toBe(1);
  });

  it('leaves a later start working after an earlier one was torn down', async () => {
    const inner = new ScopedHandler();
    const handler = withPorts(inner, portsFixture());

    handler.start().teardown();
    const second = handler.start();
    second.context.poll.start();
    await settle();

    expect(inner.calls).toBe(1);
  });

  it('closes the scope when the handler throws before returning a context', async () => {
    // The scope is reachable only through the handle a successful start returns, so
    // a handler that registers a resource and then fails would leave it running with
    // nothing left able to close it — and the container never gets a handle whose
    // teardown it could call.
    const clock = new VirtualTimeScheduler();
    let calls = 0;
    const failing: PortedAppHandler<TestPorts, ScopedContext> = {
      start(_ports, scope) {
        scope
          .poller(
            async () => {
              calls += 1;
              return calls;
            },
            { intervalMs: 100, scheduler: clock },
          )
          .start();
        throw new Error('adapter unavailable');
      },
    };

    expect(() => withPorts(failing, portsFixture()).start()).toThrow('adapter unavailable');
    await settle();
    const atThrow = calls;
    clock.flush();
    await settle();

    expect(calls).toBe(atThrow);
  });

  it('turns a context-only handle into one the container can tear down', () => {
    // `PortedAppHandle` has no teardown; `AppHandle` requires one. Bridging the two
    // is the whole reason withPorts owns the scope.
    const handle: AppHandle<ScopedContext> = withPorts(new ScopedHandler(), portsFixture()).start();

    expect(typeof handle.teardown).toBe('function');
  });
});
