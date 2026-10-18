/**
 * In-memory host for the session Code pane: a worktree's files (`worktree.WorktreeService`) and its
 * code index as `code_navigation.CodeNavigationService.CodeSession` serves it.
 *
 * A fake, not a mock: it behaves like the daemon's `CodeSession` handler.
 *
 * - A session starts on the client's `open` frame, answering with the index's current progress, and
 *   pushes every later change to every open session.
 * - While the index loads, a hover / definition / references query is **held** (`held`) and answered
 *   once the test calls `finishLoading()` — or answered with an `error`, as the daemon does, once it
 *   calls `failLoading(reason)`; a probe never waits and answers `UNKNOWN`.
 * - Once loaded, queries are answered at once from the symbols the scenario names; a probe answers
 *   `DEFINABLE` with the symbol's definition, or `NONE`. A hover on a position the scenario does not
 *   name answers no markdown, as rust-analyzer does for a token it has nothing to say about.
 * - `cancel` drops a held query.
 * - `holdingProbeAnswers()` keeps probe answers back until `answerHeldProbes()`, so a spec can act
 *   while a probe is in flight.
 * - `dropSessions(reason)` ends every open session under the pane, as a transport blip or a daemon
 *   restart does; `failingNextOpens` / `refusingEveryOpen` make the next sessions end at once.
 *
 * Every frame the pane sends is recorded, so a spec asserts what the pane asked for and cancelled.
 *
 * The fake refuses what the daemon refuses, loudly: a query without a position ends its session
 * `INVALID_ARGUMENT`, and reading a file the scenario never declared is `NOT_FOUND`.
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

/** No server holds the index: it was let go for sitting unused, and has not started again yet. */
/** A worktree with no code index at all — not a Rust worktree — for `reason`. */
export function noIndexForTheWorktree(reason: string): IndexProgressInit {
  return {
    state: CodeIndexState.NONE,
    phases: [],
    phase: "",
    percentage: 0,
    line: "",
    furthest: "",
    ready: false,
    error: reason,
  };
}

export function anIndexIdle(): IndexProgressInit {
  return {
    state: CodeIndexState.IDLE,
    phases: [],
    phase: "",
    percentage: 0,
    line: "",
    furthest: "",
    ready: false,
    error: "",
  };
}

/** The index could not load, for `reason`. */
export function anIndexFailed(reason: string): IndexProgressInit {
  return {
    state: CodeIndexState.FAILED,
    phases: [],
    phase: "",
    percentage: 0,
    line: "",
    furthest: "",
    ready: false,
    error: reason,
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
  /** Who ended the session: the pane (its request stream ended) or the host. */
  endedBy: "pane" | "host" | null;
}

interface RecordedFrame {
  readonly session: FakeSession;
  readonly frame: CodeSessionRequest;
}

/** What the daemon answers a held query with once the index failed to load. */
const failedIndexAnswer = (reason: string) => `the code index failed: ${reason}`;

interface HeldQuery {
  readonly session: FakeSession;
  readonly query: CodeQuery;
}

const keyOf = (at: SourcePoint) => `${at.relPath}:${at.line}:${at.column}`;

/** A query as a spec names it; the host refuses one without a position before it gets here. */
function askedQuery(query: CodeQuery): AskedQuery {
  if (query.position === undefined) throw new Error(`query ${query.requestId} names no position`);
  return {
    kind: CodeQueryKind[query.kind] as keyof typeof CodeQueryKind,
    relPath: query.relPath,
    line: query.position.line,
    column: query.position.column,
  };
}

export class CodeIndexHost {
  private progress: IndexProgressInit = anIndexReady();
  private readonly files: Record<string, string> = {};
  private readonly symbols = new Map<string, KnownSymbol>();
  private readonly sessions: FakeSession[] = [];
  private held: HeldQuery[] = [];
  private readonly frames: RecordedFrame[] = [];
  /** How every next session attempt fails at once, in order; a refusal fails every one. */
  private readonly openFailures: ConnectError[] = [];
  private refusal: ConnectError | null = null;
  /** Probes whose answers are kept back, while `holdingProbeAnswers` is on. */
  private probesInFlight: HeldQuery[] | null = null;
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

  /**
   * The index fails to load, for `reason`: every open session hears so, and every held query is
   * answered with the daemon's error naming it.
   */
  failLoading(reason: string): void {
    this.reportProgress(anIndexFailed(reason));
    const held = this.held;
    this.held = [];
    for (const { session, query } of held) {
      session.outbox.push(this.answerWith(query, { case: "error", value: failedIndexAnswer(reason) }));
    }
  }

  /**
   * Every open session ends under the pane with `reason` (`UNAVAILABLE`), as a transport blip or a
   * daemon restart does; what it held is gone with it.
   */
  dropSessions(reason: string): void {
    for (const session of this.openSessions()) {
      session.endedBy = "host";
      session.outbox.fail(new ConnectError(reason, Code.Unavailable));
    }
    this.held = this.held.filter(({ session }) => session.endedBy === null);
    this.probesInFlight =
      this.probesInFlight?.filter(({ session }) => session.endedBy === null) ?? null;
  }

  /** Probe answers are kept back until `answerHeldProbes()`. */
  holdingProbeAnswers(): this {
    this.probesInFlight = [];
    return this;
  }

  /**
   * Answer every probe kept back — cancelled ones too, as an answer already on the wire when the
   * pane's `cancel` arrived would be.
   */
  answerHeldProbes(): void {
    if (this.probesInFlight === null) throw new Error("the host is not holding probe answers");
    const probes = this.probesInFlight;
    this.probesInFlight = [];
    for (const { session, query } of probes) session.outbox.push(this.answer(query));
  }

  /**
   * The next sessions end at once after their `open`, one per reason, each `UNAVAILABLE` with its
   * own reason — so a spec sees, in the strip, which attempt the pane last heard fail.
   */
  failingNextOpens(...reasons: string[]): this {
    for (const reason of reasons) this.openFailures.push(new ConnectError(reason, Code.Unavailable));
    return this;
  }

  /** Every session from now on is refused with `reason` (`PERMISSION_DENIED`) after its `open`. */
  refusingEveryOpen(reason: string): this {
    this.refusal = new ConnectError(reason, Code.PermissionDenied);
    return this;
  }

  // -- observations ----------------------------------------------------------

  /** The worktree every `open` frame named, in the order the sessions opened — refused ones too. */
  openedWorktrees(): OpenedWorktree[] {
    return this.frames.flatMap(({ frame }) =>
      frame.frame.case === "open" ? [requestFields(frame.frame.value)] : [],
    );
  }

  /** Every query the pane sent, on every session, in order, without request ids. */
  queriesAsked(): AskedQuery[] {
    return this.frames.flatMap(({ frame }) =>
      frame.frame.case === "query" ? [askedQuery(frame.frame.value)] : [],
    );
  }

  /** Every query the pane cancelled, in the order it cancelled them, without request ids. */
  queriesCancelled(): AskedQuery[] {
    return this.frames.flatMap(({ session, frame }) => {
      if (frame.frame.case !== "cancel") return [];
      const cancelled = this.queryOf(session, frame.frame.value.requestId);
      if (cancelled === undefined) {
        throw new Error(`the pane cancelled request ${frame.frame.value.requestId}, which it never asked`);
      }
      return [askedQuery(cancelled)];
    });
  }

  /** How many sessions are open now (opened and not yet ended by either side). */
  openSessionCount(): number {
    return this.openSessions().length;
  }

  /** How many sessions the pane has ended. */
  endedSessionCount(): number {
    return this.sessions.filter((session) => session.endedBy === "pane").length;
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
        readWorktreeFile: (req) => {
          const content = host.files[req.relPath];
          if (content === undefined) {
            throw new ConnectError(`the scenario declares no file ${req.relPath}`, Code.NotFound);
          }
          return create(ReadWorktreeFileResponseSchema, {
            contentUtf8: content,
            truncated: false,
            byteSize: BigInt(content.length),
          });
        },
      })
      .implement(CodeNavigationService, {
        async *codeSession(requests: AsyncIterable<CodeSessionRequest>, ctx: HandlerContext) {
          const session: FakeSession = { outbox: new EventQueue(), opened: false, endedBy: null };
          host.sessions.push(session);
          const failure = host.refusal ?? host.openFailures.shift();
          if (failure !== undefined) {
            // Read the `open`, so the attempt is recorded, then end the session at once.
            for await (const frame of requests) {
              host.frames.push({ session, frame });
              break;
            }
            session.endedBy = "host";
            throw failure;
          }
          const end = () => {
            session.endedBy ??= "pane";
            session.outbox.close();
          };
          ctx.signal.addEventListener("abort", end);
          void (async () => {
            try {
              for await (const frame of requests) {
                host.frames.push({ session, frame });
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
    return this.sessions.filter((session) => session.opened && session.endedBy === null);
  }

  /** The query `requestId` named on `session`. */
  private queryOf(session: FakeSession, requestId: bigint): CodeQuery | undefined {
    return this.frames
      .flatMap((recorded) =>
        recorded.session === session && recorded.frame.frame.case === "query"
          ? [recorded.frame.frame.value]
          : [],
      )
      .find((query) => query.requestId === requestId);
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
    if (query.position === undefined) {
      session.endedBy = "host";
      session.outbox.fail(
        new ConnectError(`query ${query.requestId} names no position`, Code.InvalidArgument),
      );
      return;
    }
    if (query.kind === CodeQueryKind.PROBE && this.probesInFlight !== null) {
      this.probesInFlight.push({ session, query });
      return;
    }
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
    const symbol = this.symbols.get(keyOf(askedQuery(query)));
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
    return this.answerWith(query, result);
  }

  private answerWith(
    query: CodeQuery,
    result: MessageInitShape<typeof CodeQueryAnswerSchema>["result"],
  ): CodeSessionEvent {
    return create(CodeSessionEventSchema, {
      event: { case: "answer", value: { requestId: query.requestId, result } },
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
