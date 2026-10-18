import { useEffect, useState } from "react";
import { create, type MessageInitShape } from "@bufbuild/protobuf";
import type { Client } from "@connectrpc/connect";
import { AsyncQueue } from "tddy-livekit-web";

import {
  CodeSessionRequestSchema,
  type CodeIndexProgress,
  type CodeNavigationService,
  type CodeQueryAnswer,
  type CodeQueryKind,
  type CodeSessionEvent,
  type CodeSessionRequest,
} from "../../gen/code_navigation_pb";
import type { CodePosition } from "./codeNavigationApi";
import { codeIndexStatusOf, codeIndexStatusOfError, type CodeIndexStatus } from "./useCodeIndexProgress";

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
  | { kind: "answered"; result: CodeQueryResult }
  | { kind: "cancelled" };

/** A query in flight: its id (for `cancel`) and its outcome. Rejects when the session fails. */
export type CodeQueryHandle = { readonly id: bigint; readonly outcome: Promise<CodeQueryOutcome> };

export type AskOptions = {
  /** The host holds the query until the index has loaded; its answer follows later. */
  onHeld?: () => void;
};

/** Queries over one open `code_navigation.CodeSession`. */
export interface CodeSession {
  ask(kind: CodeQueryKind, relPath: string, at: CodePosition, options?: AskOptions): CodeQueryHandle;
  /** Drop the query `id`: the host stops holding it and its outcome resolves `cancelled`. */
  cancel(id: bigint): void;
}

type PendingQuery = {
  resolve: (outcome: CodeQueryOutcome) => void;
  reject: (error: Error) => void;
  onHeld?: () => void;
};

/**
 * The client half of one `CodeSession`: the request frames it sends and the queries awaiting their
 * answers, matched by the request id the client chose.
 */
class CodeSessionChannel implements CodeSession {
  private readonly requests = new AsyncQueue<CodeSessionRequest>();
  private readonly pending = new Map<bigint, PendingQuery>();
  private nextId = 1n;
  private ended = false;

  constructor(open: CodeSessionConfig) {
    // The first frame opens the session; the LiveKit transport opens the stream only once a client
    // frame is enqueued, so it is enqueued at once.
    this.send({ case: "open", value: open });
  }

  /** The request frames, for the call; ends when the session is closed. */
  get frames(): AsyncIterable<CodeSessionRequest> {
    return this.requests;
  }

  ask(kind: CodeQueryKind, relPath: string, at: CodePosition, options: AskOptions = {}) {
    const id = this.nextId++;
    const outcome = new Promise<CodeQueryOutcome>((resolve, reject) => {
      if (this.ended) {
        reject(new Error("The code session has ended"));
        return;
      }
      this.pending.set(id, { resolve, reject, onHeld: options.onHeld });
    });
    this.send({
      case: "query",
      value: { requestId: id, kind, relPath, position: { line: at.line, column: at.column } },
    });
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

  /** The pane let go: end the request stream; queries still waiting resolve `cancelled`. */
  close(): void {
    this.finish((query) => query.resolve({ kind: "cancelled" }));
  }

  /** The session ended under the pane: queries still waiting reject with `error`. */
  fail(error: Error): void {
    this.finish((query) => query.reject(error));
  }

  private finish(settle: (query: PendingQuery) => void): void {
    if (this.ended) return;
    this.ended = true;
    this.requests.close();
    for (const query of this.pending.values()) settle(query);
    this.pending.clear();
  }

  private send(frame: MessageInitShape<typeof CodeSessionRequestSchema>["frame"]): void {
    if (this.ended) return;
    this.requests.enqueue(create(CodeSessionRequestSchema, { frame }));
  }
}

/**
 * Hold one `code_navigation.CodeSession` open on the worktree for as long as the caller is mounted
 * (and `client` is given): opening it warms the worktree's code index, its progress streams into
 * `status`, and `session` asks hover / definition / references / probe queries on the same stream.
 *
 * A new client or worktree closes the old session and opens a fresh one.
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
    // The LiveKit transport never reads an `AbortSignal`, so closing the request stream is what ends
    // the session on every transport; the signal ends it sooner on those that do read it, and
    // `cancelled` drops whatever arrives after.
    const abort = new AbortController();
    let cancelled = false;
    (async () => {
      try {
        for await (const event of client.codeSession(channel.frames, { signal: abort.signal })) {
          if (cancelled) return;
          const progress = channel.receive(event);
          if (progress !== null) setStatus(codeIndexStatusOf(progress));
        }
        if (cancelled) return;
        const ended = new Error("the code session ended");
        channel.fail(ended);
        setStatus(codeIndexStatusOfError(ended));
      } catch (err) {
        if (cancelled) return;
        channel.fail(err instanceof Error ? err : new Error(String(err)));
        setStatus(codeIndexStatusOfError(err));
      }
    })();
    return () => {
      cancelled = true;
      channel.close();
      abort.abort();
    };
  }, [client, sessionToken, projectId, worktreePath]);

  return { status, session };
}
