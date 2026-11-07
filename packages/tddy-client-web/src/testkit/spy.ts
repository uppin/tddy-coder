/**
 * A recording function for the port doubles. Under `bun test` it is bun's own `mock`, so a spec can
 * assert on it with `expect(port.logout).toHaveBeenCalledTimes(1)`; in a Cypress bundle, where
 * `bun:test` does not exist, it is a plain function that records `.mock.calls`.
 */
// A variable specifier keeps the bundler from trying to resolve a runtime-only module.
const BUN_TEST = "bun:test";

type Spy<A extends unknown[], R> = ((...args: A) => R) & { mock: { calls: A[] } };

const bunMock: (<F extends (...args: never[]) => unknown>(impl: F) => F) | null = await import(/* @vite-ignore */ BUN_TEST)
  .then((m: { mock: <F extends (...args: never[]) => unknown>(impl: F) => F }) => m.mock)
  .catch(() => null);

export function aSpy<A extends unknown[], R>(impl: (...args: A) => R): Spy<A, R> {
  if (bunMock) return bunMock(impl) as unknown as Spy<A, R>;
  const calls: A[] = [];
  const spy = ((...args: A) => {
    calls.push(args);
    return impl(...args);
  }) as Spy<A, R>;
  spy.mock = { calls };
  return spy;
}
