/**
 * Unit tests for the terminal feed a daemon-served session offers over `TerminalSessionService`.
 *
 * The feed is one bidi `StreamSessionTerminalIO` call: the output comes down it and the keystrokes
 * go up it, and every open asks for a snapshot of the current screen. These pin the decisions that
 * live in the feed and not in the component that renders it:
 *
 *   • **A snapshot on every open.** A re-open is a fresh VT that saw nothing, so it is never asked
 *     for a byte resume — the daemon shows it the screen as it is now.
 *   • **The per-frame identity guard.** A frame stamped for another terminal is dropped rather than
 *     painted, and the stream carries on — a mis-routed frame says nothing about the ones after it.
 *   • **ACK frames are not output.** They carry an applied-input offset and no bytes, and there is
 *     nowhere on a `TerminalFrame` to put one.
 *   • **The control token is read per send.** The lease moves when another screen claims the
 *     terminal; a token snapshotted at open goes stale and every later keystroke is refused
 *     silently, since input has no reply the terminal renders.
 *
 * Technical: `packages/tddy-web/src/rpc/connections/terminalFeed.ts`
 */

import { describe, expect, it } from "bun:test";
import { create } from "@bufbuild/protobuf";
import type { Client } from "@connectrpc/connect";
import {
  SessionTerminalOutputSchema,
  StreamReplayMode,
  TerminalHistoryChunkSchema,
  type TerminalSessionService,
} from "../../gen/terminal_session_pb";
import { MAIN_TERMINAL_ID } from "../../lib/terminalFrameIdentity";
import type { HistoryChunk } from "../../lib/terminalHistoryLoader";
import type { TerminalFeed, TerminalHistoryFetcher, TerminalOptions } from "./terminal";
import { openDaemonTerminalFeed, TerminalResumePoint } from "./terminalFeed";

const A_SESSION = "session-0001";
const ANOTHER_SESSION = "session-0002";
const A_TERMINAL_ID = "bash-2";
const A_SESSION_TOKEN = "gho-alice";
const A_GRID = { cols: 120, rows: 40 };

/** How far the first open of this terminal got before it was closed. */
const AN_OFFSET_ALREADY_PAINTED = 4200n;

const bytesOf = (text: string): Uint8Array => new TextEncoder().encode(text);
const textOf = (bytes: Uint8Array): string => new TextDecoder().decode(bytes);

/** The scrollback fetcher — a daemon that holds the capture ring always offers one. */
function scrollbackOf(feed: TerminalFeed): TerminalHistoryFetcher {
  const { history } = feed;
  if (history === undefined) throw new Error("expected the feed to offer a history fetcher");
  return history;
}

/** What a fetched chunk holds — these specs always fetch one that exists. */
function textOfChunk(chunk: HistoryChunk | null): string {
  if (chunk === null) throw new Error("expected the fetcher to return a chunk");
  return textOf(chunk.data);
}

/**
 * The control lease the typing screen holds, which another screen can take from it.
 *
 * A getter rather than a value, exactly as the feed consumes it: the point of the whole design is
 * that a re-claim is visible to the *next* send without reopening the stream.
 */
function aLeaseHeldAs(token: string) {
  let held = token;
  return {
    token: (): string => held,
    reclaimedAs(next: string): void {
      held = next;
    },
  };
}

/** What a pane states when it opens this session's bash terminal. */
function terminalOptions(lease = aLeaseHeldAs("lease-one")): TerminalOptions {
  return {
    terminalId: A_TERMINAL_ID,
    sessionToken: A_SESSION_TOKEN,
    controlToken: lease.token,
    initialGrid: A_GRID,
  };
}

// ---------------------------------------------------------------------------
// Doubles
// ---------------------------------------------------------------------------

/** One `GetTerminalHistory` as the feed issues it. */
interface HistoryRequest {
  sessionToken: string;
  sessionId: string;
  terminalId: string;
  fromOffset: bigint;
  untilOffset: bigint;
  maxBytes: number;
}

type OutputFrame = ReturnType<typeof create<typeof SessionTerminalOutputSchema>>;

/** A live output frame of this session's bash terminal, stamped as the daemon stamps every frame. */
function anOutputFrameOf(text: string): OutputFrame {
  return create(SessionTerminalOutputSchema, {
    data: bytesOf(text),
    sessionId: A_SESSION,
    terminalId: A_TERMINAL_ID,
  });
}

/** What the daemon sends to confirm applied input: an offset and no bytes at all. */
function anAckFrameFor(appliedInputOffset: bigint): OutputFrame {
  return create(SessionTerminalOutputSchema, {
    ackedInputOffset: appliedInputOffset,
    sessionId: A_SESSION,
    terminalId: A_TERMINAL_ID,
  });
}

/** A frame from a different session's terminal, mis-routed onto this subscription. */
function aFrameFromAnotherTerminal(text: string): OutputFrame {
  return create(SessionTerminalOutputSchema, {
    data: bytesOf(text),
    sessionId: ANOTHER_SESSION,
    terminalId: A_TERMINAL_ID,
  });
}

/** What the daemon's capture ring replays when the terminal pages backwards. */
const RETAINED_HISTORY = "cargo build --release\r\n";

describe("a terminal resume point", () => {
  it("has received nothing before the first frame lands", () => {
    // Given a terminal nobody has opened yet
    const resume = new TerminalResumePoint();

    // Then there is no offset recorded yet
    expect({ synced: resume.synced, fromOffset: resume.fromOffset }).toEqual({
      synced: false,
      fromOffset: 0n,
    });
  });

  it("resumes from the byte it last recorded", () => {
    // Given a terminal whose open has landed an offset-anchored frame
    const resume = new TerminalResumePoint();

    // When that offset is recorded
    resume.record(AN_OFFSET_ALREADY_PAINTED);

    // Then it reads back exactly that
    expect({ synced: resume.synced, fromOffset: resume.fromOffset }).toEqual({
      synced: true,
      fromOffset: AN_OFFSET_ALREADY_PAINTED,
    });
  });
});

// ---------------------------------------------------------------------------
// The bidi stream: one call carries the output down and the keystrokes up, and every open asks
// for a snapshot of the current screen (PRD-2026-10-25-terminal-snapshot-host)
// ---------------------------------------------------------------------------

/** One `StreamSessionTerminalIO` call as the feed makes it: every message it sent up. */
interface BidiOpen {
  readonly messages: Array<{
    sessionToken: string;
    mode: StreamReplayMode;
    initialCols: number;
    initialRows: number;
    sessionId: string;
    terminalId: string;
    data: Uint8Array;
    inputOffset: bigint;
    controlToken: string;
  }>;
}

/** A daemon serving the bidi stream: answers each open with `frames`, then ends it (or holds it
 *  open, for a spec that types into a live terminal). */
function aBidiDaemonAnswering(frames: OutputFrame[], { staysOpen = false } = {}) {
  const opens: BidiOpen[] = [];
  const historyRequests: HistoryRequest[] = [];
  const client = {
    async *getTerminalHistory(req: HistoryRequest) {
      historyRequests.push(req);
      yield create(TerminalHistoryChunkSchema, {
        data: bytesOf(RETAINED_HISTORY),
        startOffset: 0n,
        endOffset: BigInt(RETAINED_HISTORY.length),
        atOldest: true,
        atEnd: true,
      });
    },
    async *streamSessionTerminalIO(requests: AsyncIterable<BidiOpen["messages"][number]>) {
      const open: BidiOpen = { messages: [] };
      opens.push(open);
      const reader = requests[Symbol.asyncIterator]();
      const first = await reader.next();
      if (!first.done) open.messages.push(first.value);
      void (async () => {
        for (let next = await reader.next(); !next.done; next = await reader.next()) {
          open.messages.push(next.value);
        }
      })();
      for (const frame of frames) yield frame;
      if (staysOpen) await new Promise<never>(() => undefined);
    },
  };
  return {
    client: client as unknown as Client<typeof TerminalSessionService>,
    opens,
    historyRequests,
  };
}

/** A snapshot frame of this session's bash terminal at sequence point `endOffset`. */
function aSnapshotAt(endOffset: bigint): OutputFrame {
  return create(SessionTerminalOutputSchema, {
    data: bytesOf("\x1bc$ "),
    snapshot: true,
    endOffset,
    snapshotRows: A_GRID.rows,
    snapshotCols: A_GRID.cols,
    sessionId: A_SESSION,
    terminalId: A_TERMINAL_ID,
  });
}

/** A live delta stamped with the offsets it occupies. */
function aDeltaFrom(startOffset: bigint, text: string): OutputFrame {
  return create(SessionTerminalOutputSchema, {
    data: bytesOf(text),
    startOffset,
    endOffset: startOffset + BigInt(text.length),
    sessionId: A_SESSION,
    terminalId: A_TERMINAL_ID,
  });
}

/** Wait for `condition`, failing after a second — the feed reads its stream on its own schedule. */
async function eventually(condition: () => boolean): Promise<void> {
  const deadline = Date.now() + 1000;
  while (!condition()) {
    if (Date.now() > deadline) throw new Error("condition not met within a second");
    await new Promise((resolve) => setTimeout(resolve, 5));
  }
}

describe("a daemon-served terminal feed over the bidi stream", () => {
  it("asks for a snapshot on every open, the first and every re-open alike", async () => {
    // Given a terminal whose first open already painted up to an anchored offset
    const resume = new TerminalResumePoint();
    const daemon = aBidiDaemonAnswering([aSnapshotAt(AN_OFFSET_ALREADY_PAINTED)]);
    openDaemonTerminalFeed({ client: daemon.client, sessionId: A_SESSION, resume, options: terminalOptions() });
    await eventually(() => daemon.opens.length === 1 && daemon.opens[0].messages.length === 1);

    // When the same terminal is opened again
    openDaemonTerminalFeed({ client: daemon.client, sessionId: A_SESSION, resume, options: terminalOptions() });
    await eventually(() => daemon.opens.length === 2 && daemon.opens[1].messages.length === 1);

    // Then both opens asked for the current screen, at the measured grid, for this terminal
    expect(
      daemon.opens.map(({ messages: [opening] }) => ({
        mode: opening.mode,
        initialCols: opening.initialCols,
        initialRows: opening.initialRows,
        terminalId: opening.terminalId,
      })),
    ).toEqual([
      { mode: StreamReplayMode.SNAPSHOT, initialCols: A_GRID.cols, initialRows: A_GRID.rows, terminalId: A_TERMINAL_ID },
      { mode: StreamReplayMode.SNAPSHOT, initialCols: A_GRID.cols, initialRows: A_GRID.rows, terminalId: A_TERMINAL_ID },
    ]);
  });

  it("hands the terminal the snapshot flagged as one, then deltas stamped with their offsets", async () => {
    // Given a daemon that answers with a snapshot and one delta after its sequence point
    const daemon = aBidiDaemonAnswering([aSnapshotAt(100n), aDeltaFrom(100n, "ls\r\n")]);
    const feed = openDaemonTerminalFeed({
      client: daemon.client,
      sessionId: A_SESSION,
      resume: new TerminalResumePoint(),
      options: terminalOptions(),
    });
    const received: Array<{ snapshot?: boolean; startOffset?: bigint; endOffset: bigint; text: string }> = [];
    feed.stream.onMessage((frame) =>
      received.push({
        snapshot: frame.snapshot,
        startOffset: frame.startOffset,
        endOffset: frame.endOffset,
        text: textOf(frame.data),
      }),
    );

    // When both frames arrive
    await eventually(() => received.length === 2);

    // Then the snapshot is marked as one and anchored at its sequence point, and the delta starts there
    expect(received).toEqual([
      { snapshot: true, startOffset: 0n, endOffset: 100n, text: "\x1bc$ " },
      { snapshot: false, startOffset: 100n, endOffset: 104n, text: "ls\r\n" },
    ]);
  });

  it("sends keystrokes up the same stream, each carrying the cumulative input offset", async () => {
    // Given an open feed
    const daemon = aBidiDaemonAnswering([aSnapshotAt(10n)]);
    const feed = openDaemonTerminalFeed({
      client: daemon.client,
      sessionId: A_SESSION,
      resume: new TerminalResumePoint(),
      options: terminalOptions(),
    });

    // When the user types twice
    feed.stream.send(bytesOf("ls"));
    feed.stream.send(bytesOf(" -la\r"));

    // Then both went up the one open stream with the running byte total and the held lease
    await eventually(() => (daemon.opens[0]?.messages.length ?? 0) >= 3);
    expect(daemon.opens).toHaveLength(1);
    expect(
      daemon.opens[0].messages.slice(1).map((m) => ({
        text: textOf(m.data),
        inputOffset: m.inputOffset,
        controlToken: m.controlToken,
      })),
    ).toEqual([
      { text: "ls", inputOffset: 2n, controlToken: "lease-one" },
      { text: " -la\r", inputOffset: 7n, controlToken: "lease-one" },
    ]);
  });

  it("opens a terminal at the measured grid, presenting the lease", async () => {
    // Given a daemon and a terminal nothing has painted yet
    const daemon = aBidiDaemonAnswering([aSnapshotAt(10n)]);
    const feed = openDaemonTerminalFeed({
      client: daemon.client,
      sessionId: A_SESSION,
      resume: new TerminalResumePoint(),
      options: terminalOptions(),
    });

    // When the stream runs
    await feed.ended;

    // Then the opening message asked for this session's bash terminal at the measured grid — the PTY
    // is resized before the snapshot, so a screen captured at one width is never re-wrapped into a
    // terminal of another — presenting the lease, without which the daemon ignores the grid
    // (watching another screen's session must not reflow it)
    expect(
      daemon.opens.map(({ messages: [opening] }) => ({
        sessionToken: opening.sessionToken,
        sessionId: opening.sessionId,
        terminalId: opening.terminalId,
        initialCols: opening.initialCols,
        initialRows: opening.initialRows,
        mode: opening.mode,
        controlToken: opening.controlToken,
      })),
    ).toEqual([
      {
        sessionToken: A_SESSION_TOKEN,
        sessionId: A_SESSION,
        terminalId: A_TERMINAL_ID,
        initialCols: A_GRID.cols,
        initialRows: A_GRID.rows,
        mode: StreamReplayMode.SNAPSHOT,
        controlToken: "lease-one",
      },
    ]);
  });

  it("drops a frame stamped for another terminal and keeps reading the stream", async () => {
    // Given a stream carrying one mis-routed frame ahead of this terminal's own
    const daemon = aBidiDaemonAnswering([
      aFrameFromAnotherTerminal("another session's output"),
      anOutputFrameOf("ls -la\r\n"),
    ]);
    const feed = openDaemonTerminalFeed({
      client: daemon.client,
      sessionId: A_SESSION,
      resume: new TerminalResumePoint(),
      options: terminalOptions(),
    });
    const painted: string[] = [];
    feed.stream.onMessage((frame) => painted.push(textOf(frame.data)));

    // When the stream runs
    await feed.ended;

    // Then the foreign bytes were never painted, and the frame after it still was
    expect(painted).toEqual(["ls -la\r\n"]);
  });

  it("does not deliver the daemon's input acknowledgements as output", async () => {
    // Given a stream carrying an ACK — this terminal's own, so only its emptiness can exclude it
    const daemon = aBidiDaemonAnswering([anAckFrameFor(12n), anOutputFrameOf("ls -la\r\n")]);
    const feed = openDaemonTerminalFeed({
      client: daemon.client,
      sessionId: A_SESSION,
      resume: new TerminalResumePoint(),
      options: terminalOptions(),
    });
    const painted: string[] = [];
    feed.stream.onMessage((frame) => painted.push(textOf(frame.data)));

    // When the stream runs
    await feed.ended;

    // Then only the output frame reached the terminal
    expect(painted).toEqual(["ls -la\r\n"]);
  });

  it("presents the lease held at the moment of each send, not the one held at open", async () => {
    // Given an open terminal whose control lease another screen then claims
    const lease = aLeaseHeldAs("lease-one");
    const daemon = aBidiDaemonAnswering([], { staysOpen: true });
    const feed = openDaemonTerminalFeed({
      client: daemon.client,
      sessionId: A_SESSION,
      resume: new TerminalResumePoint(),
      options: terminalOptions(lease),
    });

    // When a keystroke is sent on either side of the re-claim
    feed.stream.send(bytesOf("l"));
    lease.reclaimedAs("lease-two");
    feed.stream.send(bytesOf("s"));

    // Then the second keystroke carried the new lease — a token hoisted at open would have been
    // refused from here on, silently
    await eventually(() => (daemon.opens[0]?.messages.length ?? 0) >= 3);
    expect(daemon.opens[0].messages.slice(1).map((message) => message.controlToken)).toEqual([
      "lease-one",
      "lease-two",
    ]);
  });

  it("fetches scrollback from the daemon that holds the capture ring", async () => {
    // Given a feed on a daemon holding retained output
    const daemon = aBidiDaemonAnswering([]);
    const feed = openDaemonTerminalFeed({
      client: daemon.client,
      sessionId: A_SESSION,
      resume: new TerminalResumePoint(),
      options: terminalOptions(),
    });

    // When the terminal pages backwards
    const chunk = await scrollbackOf(feed)(0n, 0n);

    // Then it reads what the ring holds
    expect(textOfChunk(chunk)).toEqual(RETAINED_HISTORY);
  });

  it("authorises the history fetch with the caller's session token", async () => {
    // Given a feed opened with the caller's token. `GetTerminalHistory` is server-streaming, and
    // the unary-only auth gate does not cover it — an unstated token is refused outright.
    const daemon = aBidiDaemonAnswering([]);
    const feed = openDaemonTerminalFeed({
      client: daemon.client,
      sessionId: A_SESSION,
      resume: new TerminalResumePoint(),
      options: terminalOptions(),
    });

    // When the terminal pages backwards
    await scrollbackOf(feed)(0n, 0n);

    // Then the fetch states it, for this session's own terminal
    expect(daemon.historyRequests).toEqual([
      {
        sessionToken: A_SESSION_TOKEN,
        sessionId: A_SESSION,
        terminalId: A_TERMINAL_ID,
        fromOffset: 0n,
        untilOffset: 0n,
        maxBytes: 0,
      },
    ]);
  });

  it("opens the reserved main terminal when the pane names none", async () => {
    // Given a pane showing the Agent terminal, which asks for the empty terminal id
    const daemon = aBidiDaemonAnswering([
      create(SessionTerminalOutputSchema, {
        data: bytesOf("claude> "),
        sessionId: A_SESSION,
        // The daemon stamps the RESOLVED id back, never the empty one the request carried
        terminalId: MAIN_TERMINAL_ID,
      }),
    ]);
    const feed = openDaemonTerminalFeed({
      client: daemon.client,
      sessionId: A_SESSION,
      resume: new TerminalResumePoint(),
      options: { sessionToken: A_SESSION_TOKEN, controlToken: () => "lease-one" },
    });
    const painted: string[] = [];
    feed.stream.onMessage((frame) => painted.push(textOf(frame.data)));

    // When the stream runs
    await feed.ended;

    // Then the frame stamped `main` is this pane's own, not a foreign one
    expect(painted).toEqual(["claude> "]);
  });
});
