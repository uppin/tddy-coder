/**
 * `stubScope` doubles this package's own abstraction, so it belongs here.
 *
 * What a handler test wants to know is *which* resources a handler put in the
 * scope's care — the question the old hand-written teardown answered by being
 * readable. A double that only recorded calls would answer it while lying about
 * behaviour, so this one is a real scope that also keeps a tally.
 */
import { Subject, VirtualTimeScheduler } from 'rxjs';
import { describe, expect, it } from 'bun:test';

import type { AppScope } from '../async/index';

import { stubScope } from './index';

const clock = (): VirtualTimeScheduler => new VirtualTimeScheduler();

const settle = (): Promise<void> => Promise.resolve().then(() => undefined);

/** A handler-shaped function: given a scope, it builds what it needs. */
const buildResources = (scope: AppScope): void => {
  const errors = scope.errorChannel();
  const request = scope.request(async () => 'answer');
  const poller = scope.poller(async () => 'jobs', { intervalMs: 100, scheduler: clock() });
  scope.subscribe(poller.latest, () => {});
  scope.subscribe(errors.errors$, () => {});
  request.invoke();
};

describe('stubScope', () => {
  it('records nothing before anything is built', () => {
    const scope = stubScope();

    expect(scope.registrations).toEqual({
      request: 0,
      poller: 0,
      errorChannel: 0,
      subscription: 0,
    });
  });

  it('counts each kind of resource a handler registers', () => {
    const scope = stubScope();

    buildResources(scope);

    expect(scope.registrations).toEqual({
      request: 1,
      poller: 1,
      errorChannel: 1,
      subscription: 2,
    });
  });

  it('reports itself open until it is closed', () => {
    const scope = stubScope();
    buildResources(scope);

    expect(scope.closed).toBe(false);
  });

  it('reports itself closed afterwards', () => {
    const scope = stubScope();
    buildResources(scope);

    scope.close();

    expect(scope.closed).toBe(true);
  });

  it('hands back resources that actually work', async () => {
    // A recording mock that returned inert doubles would let a handler test pass
    // against a handler that never invokes anything.
    const scope = stubScope();

    const request = scope.request(async (name: string) => `hello ${name}`);
    request.invoke('world');
    await settle();

    expect(request.state.getState()).toEqual({ status: 'ready', value: 'hello world' });
  });

  it('disposes what it recorded', () => {
    const scope = stubScope();
    const source = new Subject<string>();
    const seen: string[] = [];
    scope.subscribe(source, value => seen.push(value));
    source.next('before');

    scope.close();
    source.next('after');

    expect(seen).toEqual(['before']);
  });
});
