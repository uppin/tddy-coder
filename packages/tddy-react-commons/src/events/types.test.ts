import { describe, expectTypeOf, it } from 'bun:test';
import type { Envelope, EventMap } from './index';

type TwinkleEvents = {
  exit: { code: number };
  callStarted: null;
  call: string;
};

describe('events types', () => {
  it('narrows envelope payload by event type', () => {
    type ExitEnvelope = Extract<Envelope<TwinkleEvents>, { type: 'exit' }>;
    type CallStartedEnvelope = Extract<Envelope<TwinkleEvents>, { type: 'callStarted' }>;
    expectTypeOf<ExitEnvelope>().toEqualTypeOf<{ type: 'exit'; payload: { code: number } }>();
    expectTypeOf<CallStartedEnvelope>().toEqualTypeOf<{ type: 'callStarted'; payload: null }>();
  });

  it('allows EventMap with heterogeneous payloads', () => {
    expectTypeOf<TwinkleEvents>().toMatchTypeOf<EventMap>();
    expectTypeOf<TwinkleEvents['exit']>().toEqualTypeOf<{ code: number }>();
    expectTypeOf<TwinkleEvents['callStarted']>().toEqualTypeOf<null>();
  });
});
