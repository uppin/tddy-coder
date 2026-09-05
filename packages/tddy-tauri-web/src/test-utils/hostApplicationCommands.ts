/**
 * The host application's command surface, recorded.
 *
 * Everything above the IPC boundary is driven through `webviewIpcDouble.ts` instead, which is the
 * right level for it. Releasing a connection is the exception: what the release *is* is a command
 * reaching the host, so the only place it can be observed is the `@tauri-apps/api/core` binding the
 * page's own bridges call. Standing in for that binding is a test-side substitution — the transport
 * has no idea it happened, and no branch in it knows about tests.
 */

import { mock } from "bun:test";

export interface HostApplicationCommands {
  /** The commands the page has invoked, in the order it invoked them. */
  invoked(): string[];
  /** The epoch of every connection the page has asked the host to forget. */
  releasedEpochs(): number[];
  /** Forget what has been recorded, so one test's commands are not another's. */
  forgetRecorded(): void;
}

/** The command a page sends to release a connection, and the field naming which one. */
const DISCONNECT_COMMAND = "tddy_rpc_disconnect";

/**
 * Stand in for the host application's command surface, and record what the page asks of it.
 *
 * Call this *before* `transport.ts` is loaded — Bun resolves a test file's static imports ahead of
 * its body, so the module under test has to be reached with `await import(...)` for the stand-in to
 * be the binding it captures.
 */
export function recordedHostApplicationCommands(): HostApplicationCommands {
  const invocations: Array<{ command: string; payload: unknown }> = [];

  mock.module("@tauri-apps/api/core", () => ({
    invoke: async (command: string, payload: unknown): Promise<void> => {
      invocations.push({ command, payload });
    },
    // The real one reaches for `window.__TAURI_INTERNALS__`, which no test has. A channel is only
    // ever handed straight back to the host here, so an inert object carries it faithfully.
    Channel: class {
      onmessage: ((frame: ArrayBuffer) => void) | null = null;
    },
  }));

  return {
    invoked: () => invocations.map(({ command }) => command),
    releasedEpochs: () =>
      invocations
        .filter(({ command }) => command === DISCONNECT_COMMAND)
        .map(({ payload }) => (payload as { clientEpoch: number }).clientEpoch),
    forgetRecorded: () => {
      invocations.length = 0;
    },
  };
}
