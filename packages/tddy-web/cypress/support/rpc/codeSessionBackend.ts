/**
 * In-memory host for the session Code pane: a worktree's files (`worktree.WorktreeService`) and its
 * code index as `code_navigation.CodeNavigationService.CodeSession` serves it.
 *
 * A fake, not a mock: it behaves like the daemon's `CodeSession` handler.
 *
 * - A session starts on the client's `open` frame, answering with the index's current progress, and
 *   pushes every later change to every open session.
 * - While the index loads, a hover / definition / references query is **held** (`held`) and answered
 *   once the test calls `finishLoading()`; a probe never waits and answers `UNKNOWN`.
 * - Once loaded, queries are answered at once from the symbols the scenario names; a probe answers
 *   `DEFINABLE` with the symbol's definition, or `NONE`.
 * - `cancel` drops a held query.
 *
 * Every frame the pane sends is recorded, so a spec asserts what the pane asked for.
 *
 * Feature: docs/ft/web/1-WIP/PRD-2026-10-18-code-index-warm-on-demand.md.
 */

import { create, type MessageInitShape } from "@bufbuild/protobuf";
import { Code, ConnectError, type HandlerContext } from "@connectrpc/connect";
import { anInMemoryRpcBackend, type InMemoryRpcBackend } from "tddy-connectrpc-testkit";
import {
  CodeIndexProgressSchema,
  CodeIndexState,
  CodeLocationSchema,
  CodeNavigationService,
  CodeQueryAnswerSchema,
  CodeQueryKind,
  CodeSessionEventSchema,
  Definability,
  SourcePositionSchema,
  SourceRangeSchema,
  type CodeLocation,
  type CodeQuery,
  type CodeSessionEvent,
  type CodeSessionRequest,
} from "../../../src/gen/code_navigation_pb";
import {
  ListWorktreeDirectoryResponseSchema,
  ReadWorktreeFileResponseSchema,
  WorktreeDirEntrySchema,
  WorktreeService,
} from "../../../src/gen/worktree_pb";
import { requestFields } from "./recordedRequests";

// ---------------------------------------------------------------------------
// Progress builders
// ---------------------------------------------------------------------------

export type IndexProgressInit = MessageInitShape<typeof CodeIndexProgressSchema>;

/** One work-done phase the language server is running, as it titled it. */
export interface IndexPhase {
  title: string;
  percentage: number;
  message: string;
}

/**
 * The index loading with `phases` running, in the order they began. The phase the server reported
 * on **most recently** is the one the single-phase fields (`phase`, `percentage`, `line`) name —
 * what is happening now — so pass it last.
 */
export function anIndexLoading(...phases: [IndexPhase, ...IndexPhase[]]): IndexProgressInit {
  const now = phases[phases.length - 1];
  return {
    state: CodeIndexState.LOADING,
    phases,
    phase: now.title,
    percentage: now.percentage,
    line: `${now.title}: ${now.message}`,
    furthest: "",
    ready: false,
    error: "",
  };
}

/** The index loaded and queryable. */
export function anIndexReady(): IndexProgressInit {
  return {
    state: CodeIndexState.READY,
    phases: [],
    phase: "",
    percentage: 100,
    line: "ready",
    furthest: "",
    ready: true,
    error: "",
  };
}

// ---------------------------------------------------------------------------
// Symbols
// ---------------------------------------------------------------------------

/** A one-based line and one-based byte column in a worktree file. */
export interface SourcePoint {
  relPath: string;
  line: number;
  column: number;
}

/** What the index knows about the identifier at `at`. */
export interface KnownSymbol {
  at: SourcePoint;
  hover?: string;
  definition?: CodeLocation[];
  references?: CodeLocation[];
}

/** A location inside the worktree spanning `startColumn..endColumn` of one line. */
export function aCodeLocation(
  relPath: string,
  line: number,
  startColumn: number,
  endColumn: number,
): CodeLocation {
  return create(CodeLocationSchema, {
    relPath,
    range: create(SourceRangeSchema, {
      start: create(SourcePositionSchema, { line, column: startColumn }),
      end: create(SourcePositionSchema, { line, column: endColumn }),
    }),
    outsideWorktree: false,
  });
}

// ---------------------------------------------------------------------------
// Observations
// ---------------------------------------------------------------------------

/** A query the pane sent, without its client-chosen request id. */
export interface AskedQuery {
  kind: keyof typeof CodeQueryKind;
  relPath: string;
  line: number;
  column: number;
}

/** The worktree an `open` frame named. */
export interface OpenedWorktree {
  sessionToken: string;
  projectId: string;
  worktreePath: string;
}

// ---------------------------------------------------------------------------
// The fake
// ---------------------------------------------------------------------------

/** An async queue of events one session's response stream drains. */
class EventQueue<T> implements AsyncIterable<T> {
  private readonly items: T[] = [];
  private wake: (() => void) | null = null;
  private closed = false;
  private failure: unknown = null;

  push(item: T): void {
    if (this.closed) return;
    this.items.push(item);
    this.notify();
  }

  close(): void {
    this.closed = true;
    this.notify();
  }

  fail(error: unknown): void {
    this.failure = error;
    this.close();
  }

  private notify(): void {
    const wake = this.wake;
    this.wake = null;
    wake?.();
  }

  async *[Symbol.asyncIterator](): AsyncIterator<T> {
    for (;;) {
      const next = this.items.shift();
      if (next !== undefined) {
        yield next;
        continue;
      }
      if (this.failure !== null) throw this.failure;
      if (this.closed) return;
      await new Promise<void>((resolve) => {
        this.wake = resolve;
      });
    }
  }
}

interface FakeSession {
  readonly outbox: EventQueue<CodeSessionEvent>;
  opened: boolean;
  ended: boolean;
}

interface HeldQuery {
  readonly session: FakeSession;
  readonly query: CodeQuery;
}

const keyOf = (at: SourcePoint) => `${at.relPath}:${at.line}:${at.column}`;

export class CodeIndexHost {
  private progress: IndexProgressInit = anIndexReady();
  private readonly files: Record<string, string> = {};
  private readonly symbols = new Map<string, KnownSymbol>();
  private readonly sessions: FakeSession[] = [];
  private held: HeldQuery[] = [];
  private readonly frames: CodeSessionRequest[] = [];
  private builtBackend: InMemoryRpcBackend | null = null;

  /** The worktree holds `relPath` with `content`; its directories are listed from the paths. */
  withFile(relPath: string, content: string): this {
    this.files[relPath] = content;
    return this;
  }

  /** The index starts out in `progress` (default: ready). */
  startingAt(progress: IndexProgressInit): this {
    this.progress = progress;
    return this;
  }

  /** The index knows `symbol`: its hover, definition and references. */
  knowing(symbol: KnownSymbol): this {
    this.symbols.set(keyOf(symbol.at), symbol);
    return this;
  }

  // -- runtime controls ------------------------------------------------------

  /** The index reports `progress` to every open session. */
  reportProgress(progress: IndexProgressInit): void {
    this.progress = progress;
    this.broadcastProgress();
  }

  /** The index finishes loading: every open session hears so, and every held query is answered. */
  finishLoading(): void {
    this.reportProgress(anIndexReady());
    const held = this.held;
    this.held = [];
    for (const { session, query } of held) session.outbox.push(this.answer(query));
  }

  /** Every open session's stream fails with `message`. */
  failSessions(message: string): void {
    for (const session of this.openSessions()) {
      session.outbox.fail(new ConnectError(message, Code.Unavailable));
    }
  }

  // -- observations ----------------------------------------------------------

  /** The worktree every `open` frame named, in the order the sessions opened. */
  openedWorktrees(): OpenedWorktree[] {
    return this.frames.flatMap((frame) =>
      frame.frame.case === "open" ? [requestFields(frame.frame.value)] : [],
    );
  }

  /** Every query the pane sent, in order, without request ids. */
  queriesAsked(): AskedQuery[] {
    return this.frames.flatMap((frame) =>
      frame.frame.case === "query"
        ? [
            {
              kind: CodeQueryKind[frame.frame.value.kind] as keyof typeof CodeQueryKind,
              relPath: frame.frame.value.relPath,
              line: frame.frame.value.position?.line ?? 0,
              column: frame.frame.value.position?.column ?? 0,
            },
          ]
        : [],
    );
  }

  /** How many sessions are open now (opened and not yet ended by the client). */
  openSessionCount(): number {
    return this.openSessions().length;
  }

  /** How many sessions the client has ended. */
  endedSessionCount(): number {
    return this.sessions.filter((session) => session.ended).length;
  }

  // -- the backend -----------------------------------------------------------

  /**
   * The in-memory backend serving this host, added to `base` (default: an empty backend) — pass a
   * backend that serves the rest of a larger screen. Built once.
   */
  backend(base: InMemoryRpcBackend = anInMemoryRpcBackend()): InMemoryRpcBackend {
    if (this.builtBackend !== null) return this.builtBackend;
    const host = this;
    this.builtBackend = base
      .implement(WorktreeService, {
        listWorktreeDirectory: (req) =>
          create(ListWorktreeDirectoryResponseSchema, {
            entries: host
              .directoryEntries(req.relPath)
              .map((entry) => create(WorktreeDirEntrySchema, entry)),
          }),
        readWorktreeFile: (req) =>
          create(ReadWorktreeFileResponseSchema, {
            contentUtf8: host.files[req.relPath] ?? "",
            truncated: false,
            byteSize: BigInt((host.files[req.relPath] ?? "").length),
          }),
      })
      .implement(CodeNavigationService, {
        async *codeSession(requests: AsyncIterable<CodeSessionRequest>, ctx: HandlerContext) {
          const session: FakeSession = { outbox: new EventQueue(), opened: false, ended: false };
          host.sessions.push(session);
          const end = () => {
            session.ended = true;
            session.outbox.close();
          };
          ctx.signal.addEventListener("abort", end);
          void (async () => {
            try {
              for await (const frame of requests) {
                host.frames.push(frame);
                host.handle(session, frame);
              }
            } catch {
              // The client went away mid-frame; the session ends below either way.
            }
            end();
          })();
          yield* session.outbox;
        },
      });
    return this.builtBackend;
  }

  // -- internals -------------------------------------------------------------

  private openSessions(): FakeSession[] {
    return this.sessions.filter((session) => session.opened && !session.ended);
  }

  private broadcastProgress(): void {
    for (const session of this.openSessions()) session.outbox.push(this.progressEvent());
  }

  private progressEvent(): CodeSessionEvent {
    return create(CodeSessionEventSchema, {
      event: { case: "progress", value: create(CodeIndexProgressSchema, this.progress) },
    });
  }

  private loaded(): boolean {
    return this.progress.state === CodeIndexState.READY;
  }

  private handle(session: FakeSession, frame: CodeSessionRequest): void {
    switch (frame.frame.case) {
      case "open":
        session.opened = true;
        session.outbox.push(this.progressEvent());
        return;
      case "cancel": {
        const requestId = frame.frame.value.requestId;
        this.held = this.held.filter(
          (held) => held.session !== session || held.query.requestId !== requestId,
        );
        return;
      }
      case "query":
        this.ask(session, frame.frame.value);
        return;
      default:
        return;
    }
  }

  private ask(session: FakeSession, query: CodeQuery): void {
    if (query.kind === CodeQueryKind.PROBE || this.loaded()) {
      session.outbox.push(this.answer(query));
      return;
    }
    this.held.push({ session, query });
    session.outbox.push(
      create(CodeSessionEventSchema, {
        event: { case: "held", value: { requestId: query.requestId } },
      }),
    );
  }

  private answer(query: CodeQuery): CodeSessionEvent {
    const symbol = this.symbols.get(
      keyOf({
        relPath: query.relPath,
        line: query.position?.line ?? 0,
        column: query.position?.column ?? 0,
      }),
    );
    const state = CodeIndexState.READY;
    const result = ((): MessageInitShape<typeof CodeQueryAnswerSchema>["result"] => {
      switch (query.kind) {
        case CodeQueryKind.HOVER:
          return { case: "hover", value: { markdown: symbol?.hover, state } };
        case CodeQueryKind.DEFINITION:
          return { case: "definition", value: { locations: symbol?.definition ?? [], state } };
        case CodeQueryKind.REFERENCES:
          return { case: "references", value: { locations: symbol?.references ?? [], state } };
        case CodeQueryKind.PROBE:
          return { case: "probe", value: this.probe(symbol) };
        default:
          return { case: "error", value: `unsupported query kind ${query.kind}` };
      }
    })();
    return create(CodeSessionEventSchema, {
      event: {
        case: "answer",
        value: { requestId: query.requestId, result },
      },
    });
  }

  private probe(symbol: KnownSymbol | undefined) {
    if (!this.loaded()) return { definability: Definability.UNKNOWN, locations: [] };
    const definition = symbol?.definition ?? [];
    return definition.length > 0
      ? { definability: Definability.DEFINABLE, locations: definition }
      : { definability: Definability.NONE, locations: [] };
  }

  private directoryEntries(relPath: string): Array<{ name: string; isDir: boolean }> {
    const prefix = relPath === "" ? "" : `${relPath}/`;
    const children = new Map<string, boolean>();
    for (const path of Object.keys(this.files)) {
      if (!path.startsWith(prefix)) continue;
      const [name, ...rest] = path.slice(prefix.length).split("/");
      children.set(name, rest.length > 0 || children.get(name) === true);
    }
    return [...children.entries()]
      .sort(([a], [b]) => a.localeCompare(b))
      .map(([name, isDir]) => ({ name, isDir }));
  }
}

/** A host whose worktree and code index the scenario fills in. */
export function aCodeIndexHost(): CodeIndexHost {
  return new CodeIndexHost();
}
