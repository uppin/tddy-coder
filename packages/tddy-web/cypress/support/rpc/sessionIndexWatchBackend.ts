/**
 * In-memory host for the session header's code-index indicator: `code_navigation.WatchCodeIndex`
 * as the daemon serves it.
 *
 * A fake, not a mock: it behaves like the daemon's handler, which only ever watches.
 *
 * - With no index daemon running, a watch ends at once with no message — the header's watch never
 *   starts one.
 * - Once something else starts a load (`indexDaemonStartsLoading`), every watch opened from then on
 *   hears the index's current progress and every later change.
 * - `indexDaemonStops()` ends every open watch, as the index daemon's process going away does.
 * - `refusingEveryWatch(reason)` refuses every watch (`PERMISSION_DENIED`).
 *
 * Feature: docs/ft/web/1-WIP/PRD-2026-10-18-code-index-warm-on-demand.md (session header).
 */

import { create } from "@bufbuild/protobuf";
import { Code, ConnectError } from "@connectrpc/connect";
import { anInMemoryRpcBackend, type InMemoryRpcBackend } from "tddy-connectrpc-testkit";
import {
  CodeIndexProgressSchema,
  CodeNavigationService,
  type CodeIndexProgress,
} from "../../../src/gen/code_navigation_pb";
import { EventQueue, type IndexProgressInit } from "./codeSessionBackend";

export class SessionIndexWatchHost {
  /** The index's progress while an index daemon runs; `null` while none does. */
  private progress: IndexProgressInit | null = null;
  private openWatches: EventQueue<CodeIndexProgress>[] = [];
  private watchCount = 0;
  private refusal: ConnectError | null = null;
  private builtBackend: InMemoryRpcBackend | null = null;

  /** The index daemon is running from the start, its index at `progress`. */
  withTheIndexDaemonAt(progress: IndexProgressInit): this {
    this.progress = progress;
    return this;
  }

  /** Every watch is refused with `reason` (`PERMISSION_DENIED`). */
  refusingEveryWatch(reason: string): this {
    this.refusal = new ConnectError(reason, Code.PermissionDenied);
    return this;
  }

  // -- runtime controls ------------------------------------------------------

  /** Something else — the Code pane, the agent — starts the index daemon loading, at `progress`. */
  indexDaemonStartsLoading(progress: IndexProgressInit): void {
    this.progress = progress;
    for (const watch of this.openWatches) watch.push(create(CodeIndexProgressSchema, progress));
  }

  /** The index daemon's process goes away: every open watch ends, and later ones end empty. */
  indexDaemonStops(): void {
    this.progress = null;
    for (const watch of this.openWatches) watch.close();
    this.openWatches = [];
  }

  // -- observations ----------------------------------------------------------

  /** How many watches the header has opened — ended and refused ones too. */
  watchesOpened(): number {
    return this.watchCount;
  }

  // -- the backend -----------------------------------------------------------

  /** The in-memory backend serving this host. Built once. */
  backend(): InMemoryRpcBackend {
    if (this.builtBackend !== null) return this.builtBackend;
    const host = this;
    this.builtBackend = anInMemoryRpcBackend().implement(CodeNavigationService, {
      async *watchCodeIndex() {
        host.watchCount += 1;
        if (host.refusal !== null) throw host.refusal;
        if (host.progress === null) return;
        const watch = new EventQueue<CodeIndexProgress>();
        watch.push(create(CodeIndexProgressSchema, host.progress));
        host.openWatches.push(watch);
        yield* watch;
      },
    });
    return this.builtBackend;
  }
}

/** A host with no index daemon running. */
export function aSessionIndexWatchHost(): SessionIndexWatchHost {
  return new SessionIndexWatchHost();
}

/** Retried until `host` has seen exactly `count` watches (or the command times out). */
export function expectTheHeaderToHaveWatched(host: SessionIndexWatchHost, count: number) {
  cy.wrap(null).should(() => expect(host.watchesOpened()).to.equal(count));
}

/**
 * `host` has still seen exactly `count` watches, checked only after a real macrotask: what a
 * `cy.tick` set off runs as promise continuations, which a retried assertion can otherwise outrun
 * and pass on before an early watch has landed.
 */
export function expectTheHeaderToHaveStillWatched(host: SessionIndexWatchHost, count: number) {
  cy.wait(0);
  expectTheHeaderToHaveWatched(host, count);
}
