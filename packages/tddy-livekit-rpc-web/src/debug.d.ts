/**
 * Minimal ambient types for the `debug` package (https://www.npmjs.com/package/debug).
 *
 * We depend on `debug` directly but avoid pulling `@types/debug` (a network fetch in bun install);
 * this covers exactly the surface used by `src/transport.ts`. Mirrors the real package's
 * value+namespace merge so both `createDebug(ns)` and the `createDebug.Debugger` type resolve
 * under `export =`. Mirrors `packages/tddy-web/src/debug.d.ts`.
 */
declare namespace debug {
  interface Debugger {
    (formatter: unknown, ...args: unknown[]): void;
    enabled: boolean;
    namespace: string;
    extend(namespace: string, delimiter?: string): Debugger;
  }

  interface Debug {
    (namespace: string): Debugger;
    enable(namespaces: string): void;
    disable(): string;
    enabled(namespaces: string): boolean;
  }
}

declare const debug: debug.Debug;

declare module "debug" {
  export = debug;
}
