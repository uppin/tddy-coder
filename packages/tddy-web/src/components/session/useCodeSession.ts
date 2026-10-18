import { useEffect, useState } from "react";
import { create, type MessageInitShape } from "@bufbuild/protobuf";
import type { Client } from "@connectrpc/connect";
import { AsyncQueue } from "tddy-livekit-web";

import {
  CodeQuerySchema,
  CodeSessionRequestSchema,
  type CodeIndexProgress,
  type CodeNavigationService,
  type CodeQueryAnswer,
  type CodeQueryKind,
  type CodeSessionEvent,
  type CodeSessionRequest,
} from "../../gen/code_navigation_pb";
import type { CodePosition } from "./codeNavigationApi";
import { codeIndexStatusOf, type CodeIndexStatus } from "./useCodeIndexProgress";
import { StreamReopener, streamEndOf, type StreamEnd } from "./streamReopening";

/** The worktree a code session is opened on, as the daemon authorises it. */
export type CodeSessionConfig = {
  sessionToken: string;
  projectId: string;
  worktreePath: string;
};

/** What the host answered a query with: hover, definition, references, probe, or an error. */
export type CodeQueryResult = CodeQueryAnswer["result"];

/** How a query ended: answered, or cancelled by the pane before its answer arrived. */
export type CodeQueryOutcome =
  { kind: "answered"; result: CodeQueryResult } | { kind: "cancelled" };

/**
 * A query in flight: its id (for `cancel`) and its outcome. A query outlives a dropped stream — it
 * is asked again on the reopened one — so its outcome rejects only when the host refused the
 * session, or when it is asked of a session the pane has already let go.
 */
export type CodeQueryHandle = {
  readonly id: bigint;
  readonly outcome: Promise<CodeQueryOutcome>;
};

export type AskOptions = {
  /** The host holds the query until the index has loaded; its answer follows later. */
  onHeld?: () => void;
};

/** Queries over the pane's `code_navigation.CodeSession`, across every reopening of its stream. */
export interface CodeSession {
  ask(
    kind: CodeQueryKind,
    relPath: string,
    at: CodePosition,
    options?: AskOptions,
  ): CodeQueryHandle;
  /** Drop the query `id`: the host stops holding it and its outcome resolves `cancelled`. */
  cancel(id: bigint): void;
}

type QueryFrame = MessageInitShape<typeof CodeQuerySchema>;

type PendingQuery = {
  query: QueryFrame;
  resolve: (outcome: CodeQueryOutcome) => void;
  reject: (error: Error) => void;
  onHeld?: () => void;
};

/**
 * The client half of the pane's `CodeSession`: the request frames of the stream open now, and the
 * queries awaiting their answers, matched by the request id the client chose. The queries outlive a
 * stream: `connect` opens a fresh one and asks every query still waiting again.
 */
class CodeSessionChannel implements CodeSession {
  private requests: AsyncQueue<CodeSessionRequest> | null = null;
  private readonly pending = new Map<bigint, PendingQuery>();
  private nextId = 1n;
  private closed = false;

  constructor(private readonly config: CodeSessionConfig) {}

  /**
   * The request frames of a new stream: `open` first — enqueued at once, since the LiveKit
   * transport opens the stream only once a client frame is enqueued — then every query still
   * waiting, in the order it was asked.
   */
  connect(): AsyncIterable<CodeSessionRequest> {
    this.requests?.close();
    const requests = new AsyncQueue<CodeSessionRequest>();
    this.requests = requests;
    this.send({ case: "open", value: this.config });
    for (const { query } of this.pending.values()) this.send({ case: "query", value: query });
    return requests;
  }

  /** The stream ended under the pane: stop sending on it. Waiting queries stay for the next one. */
  disconnect(): void {
    this.requests?.close();
    this.requests = null;
  }

  ask(kind: CodeQueryKind, relPath: string, at: CodePosition, options: AskOptions = {}) {
    const id = this.nextId++;
    if (this.closed) {
      return {
        id,
        outcome: Promise.reject(new Error("The code session has ended")),
      };
    }
    const query: QueryFrame = {
      requestId: id,
      kind,
      relPath,
      position: { line: at.line, column: at.column },
    };
    const outcome = new Promise<CodeQueryOutcome>((resolve, reject) => {
      this.pending.set(id, { query, resolve, reject, onHeld: options.onHeld });
    });
    this.send({ case: "query", value: query });
    return { id, outcome };
  }

  cancel(id: bigint): void {
    const query = this.pending.get(id);
    if (query === undefined) return;
    this.pending.delete(id);
    this.send({ case: "cancel", value: { requestId: id } });
    query.resolve({ kind: "cancelled" });
  }

  /** Route one event: settle or notify the query it names; a progress event is returned. */
  receive(event: CodeSessionEvent): CodeIndexProgress | null {
    switch (event.event.case) {
      case "progress":
        return event.event.value;
      case "held":
        this.pending.get(event.event.value.requestId)?.onHeld?.();
        return null;
      case "answer": {
        const { requestId, result } = event.event.value;
        const query = this.pending.get(requestId);
        this.pending.delete(requestId);
        query?.resolve({ kind: "answered", result });
        return null;
      }
      case undefined:
        return null;
    }
  }

  /** The pane let go: end the stream; queries still waiting resolve `cancelled`. */
  close(): void {
    this.finish((query) => query.resolve({ kind: "cancelled" }));
  }

  /** The host refused the session: queries still waiting reject with `error`. */
  fail(error: Error): void {
    this.finish((query) => query.reject(error));
  }

  private finish(settle: (query: PendingQuery) => void): void {
    if (this.closed) return;
    this.closed = true;
    this.disconnect();
    for (const query of this.pending.values()) settle(query);
    this.pending.clear();
  }

  private send(frame: MessageInitShape<typeof CodeSessionRequestSchema>["frame"]): void {
    this.requests?.enqueue(create(CodeSessionRequestSchema, { frame }));
  }
}

/**
 * Follows `channel` over `client`'s `CodeSession` streams from `start` to `stop`: delivers each
 * stream's events, and reopens one that ended under the pane on `StreamReopener`'s schedule —
 * unless the host refused the session, which fails the channel. `onStatus` hears progress and how
 * a stream ended.
 *
 * The LiveKit transport never reads an `AbortSignal`, so closing the request stream (`channel.close`)
 * is what ends the session on every transport; the signal ends it sooner on those that do read it,
 * and `stopped` drops whatever arrives after.
 */
class CodeSessionFollower {
  private stopped = false;
  private abort: AbortController | null = null;
  private readonly reopener = new StreamReopener();

  constructor(
    private readonly client: Client<typeof CodeNavigationService>,
    private readonly channel: CodeSessionChannel,
    private readonly onStatus: (status: CodeIndexStatus) => void,
  ) {}

  start(): void {
    void this.follow();
  }

  stop(): void {
    this.stopped = true;
    this.reopener.stop();
    this.channel.close();
    this.abort?.abort();
  }

  private async follow(): Promise<void> {
    const ended = await this.stream();
    if (ended === null || this.stopped) return;
    if (ended.refused) {
      this.channel.fail(new Error(ended.reason));
      this.onStatus({ kind: "failed", reason: ended.reason });
      return;
    }
    this.channel.disconnect();
    this.onStatus({ kind: "reconnecting", reason: ended.reason });
    this.reopener.reopenLater(() => void this.follow());
  }

  /** Read one stream to its end: how it ended, or `null` once stopped. */
  private async stream(): Promise<StreamEnd | null> {
    this.abort = new AbortController();
    try {
      const events = this.client.codeSession(this.channel.connect(), { signal: this.abort.signal });
      this.reopener.opened();
      for await (const event of events) {
        if (this.stopped) return null;
        this.deliver(event);
      }
      return { reason: "the code session ended", refused: false };
    } catch (err) {
      return streamEndOf(err);
    }
  }

  private deliver(event: CodeSessionEvent): void {
    this.reopener.heard();
    const progress = this.channel.receive(event);
    if (progress !== null) this.onStatus(codeIndexStatusOf(progress));
  }
}

/**
 * Hold one `code_navigation.CodeSession` open on the worktree for as long as the caller is mounted
 * (and `client` is given): opening it warms the worktree's code index, its progress streams into
 * `status`, and `session` asks hover / definition / references / probe queries on the same stream.
 *
 * A stream that ends or fails under the pane is reopened — after 1 s, then 2 s, 4 s … up to 30 s
 * between attempts, back to 1 s only once a stream has delivered and stayed open for 10 s
 * (`streamReopening.ts`), so a host that accepts and drops at once is backed off — with `status`
 * reading `reconnecting` meanwhile and every query still waiting asked again on the new stream. A refusal
 * (the host answered, e.g. permission denied) is not retried: `status` reads `failed`. Unmounting
 * stops retrying. A new client or worktree closes the old session and opens a fresh one.
 */
export function useCodeSession(
  client: Client<typeof CodeNavigationService> | null,
  { sessionToken, projectId, worktreePath }: CodeSessionConfig,
): { status: CodeIndexStatus; session: CodeSession | null } {
  const [status, setStatus] = useState<CodeIndexStatus>({ kind: "waiting" });
  const [session, setSession] = useState<CodeSession | null>(null);

  useEffect(() => {
    setStatus({ kind: "waiting" });
    if (client === null) {
      setSession(null);
      return;
    }
    const channel = new CodeSessionChannel({ sessionToken, projectId, worktreePath });
    setSession(channel);
    const follower = new CodeSessionFollower(client, channel, setStatus);
    follower.start();
    return () => follower.stop();
  }, [client, sessionToken, projectId, worktreePath]);

  return { status, session };
}
