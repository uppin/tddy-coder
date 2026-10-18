import { CodeQueryKind, Definability } from "../../gen/code_navigation_pb";
import { toLocationRef, type CodeLocationRef, type CodePosition } from "./codeNavigationApi";
import type { CodeSession } from "./useCodeSession";

/**
 * Whether the identifier at a position of the previewed file has a definition, and a way to hear
 * when that changes — read by each identifier on its own, so a probe answer re-renders only the
 * identifier it is about.
 */
export interface DefinableSource {
  subscribe(onChange: () => void): () => void;
  isDefinable(at: CodePosition): boolean;
}

/** A source under which nothing is definable: the preview is not navigable. */
export const NOTHING_DEFINABLE: DefinableSource = {
  subscribe: () => () => {},
  isDefinable: () => false,
};

const keyOf = (relPath: string, at: CodePosition) => `${relPath}:${at.line}:${at.column}`;

/**
 * Probes ("is there a definition here?") over one `CodeSession`, remembered per file and position
 * so the preview can underline what a click would go to.
 *
 * - A position is probed once: while its probe is in flight, or once answered, it is not asked again.
 * - A probe the loading index could not answer (`UNKNOWN`), or that failed, is not remembered, so
 *   the next dwell asks again.
 * - `forget` drops every answer, cancels every probe in flight on the host, and ignores whatever
 *   those probes still answer — a file read again may have moved its symbols.
 */
export class ProbeStore {
  private readonly answers = new Map<string, CodeLocationRef[]>();
  private readonly inFlight = new Map<string, bigint>();
  private readonly listeners = new Set<() => void>();
  /** Bumped by `forget`: an answer to a probe asked before it is dropped. */
  private generation = 0;

  constructor(private readonly session: CodeSession) {}

  /** Ask whether the identifier at `at` in `relPath` has a definition, unless already known or asked. */
  probe(relPath: string, at: CodePosition): void {
    const key = keyOf(relPath, at);
    if (this.answers.has(key) || this.inFlight.has(key)) return;
    const generation = this.generation;
    const handle = this.session.ask(CodeQueryKind.PROBE, relPath, at);
    this.inFlight.set(key, handle.id);
    const settle = (locations: CodeLocationRef[] | null) => {
      if (generation !== this.generation) return;
      this.inFlight.delete(key);
      if (locations === null) return;
      this.answers.set(key, locations);
      if (locations.length > 0) this.notify();
    };
    handle.outcome.then(
      (outcome) => {
        if (outcome.kind !== "answered" || outcome.result.case !== "probe") return settle(null);
        const { definability, locations } = outcome.result.value;
        if (definability === Definability.DEFINABLE) return settle(locations.map(toLocationRef));
        settle(definability === Definability.NONE ? [] : null);
      },
      // A failed probe leaves the identifier plain; the session's failure is the strip's to show.
      () => settle(null),
    );
  }

  /** The definitions a probe found for `at` in `relPath`; `undefined` when not (yet) known. */
  definitionsAt(relPath: string, at: CodePosition): CodeLocationRef[] | undefined {
    return this.answers.get(keyOf(relPath, at));
  }

  /** Drop every answer and cancel every probe in flight; their late answers are ignored. */
  forget(): void {
    this.generation += 1;
    for (const id of this.inFlight.values()) this.session.cancel(id);
    this.inFlight.clear();
    const hadDefinable = [...this.answers.values()].some((found) => found.length > 0);
    this.answers.clear();
    if (hadDefinable) this.notify();
  }

  /** The definability of `relPath`'s identifiers, for the preview showing it. */
  sourceFor(relPath: string): DefinableSource {
    return {
      subscribe: (onChange) => {
        this.listeners.add(onChange);
        return () => this.listeners.delete(onChange);
      },
      isDefinable: (at) => (this.answers.get(keyOf(relPath, at))?.length ?? 0) > 0,
    };
  }

  private notify(): void {
    for (const listener of this.listeners) listener();
  }
}
