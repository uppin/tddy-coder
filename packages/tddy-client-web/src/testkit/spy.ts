/**
 * A recording function for the port doubles: it runs `impl` and records each call's arguments in
 * `.mock.calls`. It is the same plain function under `bun test` and in a Cypress bundle, so a spec
 * asserts on the recorded calls (`expect(port.logout.mock.calls).toHaveLength(1)`) in either runner.
 */
export type Spy<A extends unknown[], R> = ((...args: A) => R) & { mock: { calls: A[] } };

export function aSpy<A extends unknown[], R>(impl: (...args: A) => R): Spy<A, R> {
  const calls: A[] = [];
  const spy = ((...args: A) => {
    calls.push(args);
    return impl(...args);
  }) as Spy<A, R>;
  spy.mock = { calls };
  return spy;
}
