import type { ReactElement } from 'react';
import { describe, expect, expectTypeOf, it } from 'bun:test';

import { ReactAppContainer, useAppContext, type AppHandle, type AppHandler } from './index';

interface SampleContext {
  title: string;
  ready: boolean;
}

const sampleHandler: AppHandler<SampleContext> = {
  start: () => ({
    context: { title: 'sample', ready: false },
    teardown: () => {},
  }),
};

describe('app types', () => {
  it('splits the handle into context and teardown (AT5)', () => {
    expectTypeOf<AppHandle<SampleContext>['context']>().toEqualTypeOf<SampleContext>();
    expectTypeOf<AppHandle<SampleContext>['teardown']>().toEqualTypeOf<() => void>();
  });

  it('requires start() as the only handler member (AT5)', () => {
    expectTypeOf<keyof AppHandler<SampleContext>>().toEqualTypeOf<'start'>();
    expectTypeOf(sampleHandler.start()).toEqualTypeOf<AppHandle<SampleContext>>();
  });

  it('infers Context through create and returns a renderable element (AT5)', () => {
    expectTypeOf(ReactAppContainer.create(sampleHandler, null)).toEqualTypeOf<ReactElement>();
  });

  it('returns the requested Context from useAppContext (AT5)', () => {
    expectTypeOf<ReturnType<typeof useAppContext<SampleContext>>>().toEqualTypeOf<SampleContext>();
    expectTypeOf<ReturnType<typeof useAppContext<SampleContext>>>().not.toBeAny();
  });

  it('keeps AppHandler and AppHandle type-only at runtime (AT5)', () => {
    expect(typeof ReactAppContainer.create).toBe('function');
    expect(typeof useAppContext).toBe('function');
  });
});
