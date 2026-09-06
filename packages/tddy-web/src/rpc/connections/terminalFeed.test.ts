/**
 * Unit tests for the terminal feed a daemon-served session offers over `ConnectionService`.
 *
 * This is `GrpcSessionTerminal`'s stream construction, moved onto the connection, and these pin the
 * four decisions that moved with it — each of which was a bug the component had already paid for:
 *
 *   • **TAIL once, then FROM_OFFSET.** A re-open that asked for a tail again would be handed a
 *     replay the terminal has already painted.
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
  type ConnectionService,
  SessionTerminalOutputSchema,
  StreamReplayMode,
  TerminalHistoryChunkSchema,
} from "../../gen/connection_pb";
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

/** One `StreamTerminalOutput` open as the feed issues it. */
interface OutputOpen {
  sessionToken: string;
  sessionId: string;
  terminalId: string;
  initialCols: number;
  initialRows: number;
  mode: StreamReplayMode;
  fromOffset: bigint;
}

/** One `SendTerminalInput` as the feed issues it. */
interface InputSend {
  sessionToken: string;
  sessionId: string;
  terminalId: string;
  data: Uint8Array;
  controlToken: string;
  inputOffset: bigint;
}

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

/** The offset-anchored replay frame the daemon sends first on a TAIL open. */
function aReplayFrameAt(endOffset: bigint): OutputFrame {
  return create(SessionTerminalOutputSchema, {
    data: bytesOf("$ "),
    endOffset,
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

/**
 * The daemon that owns the session: it replays `frames` on each output stream open and then closes
 * it, recording every request it was handed.
 */
function aDaemonReplaying(frames: OutputFrame[]) {
  const opens: OutputOpen[] = [];
  const sends: InputSend[] = [];
  const historyRequests: HistoryRequest[] = [];
  const client = {
    async *streamTerminalOutput(req: OutputOpen) {
      opens.push(req);
      for (const frame of frames) yield frame;
    },
    async sendTerminalInput(req: InputSend) {
      sends.push(req);
      return {};
    },
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
  };
  return { client: client as unknown as Client<typeof ConnectionService>, opens, sends, historyRequests };
}

describe("a terminal resume point", () => {
  it("has received nothing before the first frame lands", () => {
    // Given a terminal nobody has opened yet
    const resume = new TerminalResumePoint();

    // Then there is no offset to resume from, so the next open must ask for a tail
    expect({ synced: resume.synced, fromOffset: resume.fromOffset }).toEqual({
      synced: false,
      fromOffset: 0n,
    });
  });

  it("resumes from the byte it last recorded", () => {
    // Given a terminal whose open has landed its offset-anchored frame
    const resume = new TerminalResumePoint();

    // When that offset is recorded
    resume.record(AN_OFFSET_ALREADY_PAINTED);

    // Then the next open resumes from exactly there
    expect({ synced: resume.synced, fromOffset: resume.fromOffset }).toEqual({
      synced: true,
      fromOffset: AN_OFFSET_ALREADY_PAINTED,
    });
  });
});

describe("a daemon-served terminal feed", () => {
  it("opens a terminal nothing has painted yet at the live tail", async () => {
    // Given a daemon and a terminal at no offset
    const daemon = aDaemonReplaying([anOutputFrameOf("ls -la\r\n")]);
    const feed = openDaemonTerminalFeed({
      client: daemon.client,
      sessionId: A_SESSION,
      resume: new TerminalResumePoint(),
      options: terminalOptions(),
    });

    // When the stream runs
    await feed.ended;

    // Then the open asked for a tail of this session's bash terminal, at the measured grid — the
    // PTY is resized before the replay, which is what stopped a buffer captured at one width being
    // re-wrapped into a terminal of another
    expect(daemon.opens).toEqual([
      {
        sessionToken: A_SESSION_TOKEN,
        sessionId: A_SESSION,
        terminalId: A_TERMINAL_ID,
        initialCols: A_GRID.cols,
        initialRows: A_GRID.rows,
        mode: StreamReplayMode.TAIL,
        fromOffset: 0n,
      },
    ]);
  });

  it("re-opens from the byte the previous open reached", async () => {
    // Given a terminal whose first open painted up to an anchored frame
    const resume = new TerminalResumePoint();
    const daemon = aDaemonReplaying([aReplayFrameAt(AN_OFFSET_ALREADY_PAINTED)]);
    await openDaemonTerminalFeed({
      client: daemon.client,
      sessionId: A_SESSION,
      resume,
      options: terminalOptions(),
    }).ended;

    // When the same terminal is opened again
    await openDaemonTerminalFeed({
      client: daemon.client,
      sessionId: A_SESSION,
      resume,
      options: terminalOptions(),
    }).ended;

    // Then the daemon is asked only for the gap, so the replay is not painted twice
    expect(daemon.opens.map((open) => ({ mode: open.mode, fromOffset: open.fromOffset }))).toEqual([
      { mode: StreamReplayMode.TAIL, fromOffset: 0n },
      { mode: StreamReplayMode.FROM_OFFSET, fromOffset: AN_OFFSET_ALREADY_PAINTED },
    ]);
  });

  it("drops a frame stamped for another terminal and keeps reading the stream", async () => {
    // Given a stream carrying one mis-routed frame ahead of this terminal's own
    const daemon = aDaemonReplaying([
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
    const daemon = aDaemonReplaying([anAckFrameFor(12n), anOutputFrameOf("ls -la\r\n")]);
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
    const daemon = aDaemonReplaying([]);
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
    expect(daemon.sends.map((send) => send.controlToken)).toEqual(["lease-one", "lease-two"]);
  });

  it("counts each send onto the terminal's cumulative input offset", async () => {
    // Given an open terminal
    const daemon = aDaemonReplaying([]);
    const feed = openDaemonTerminalFeed({
      client: daemon.client,
      sessionId: A_SESSION,
      resume: new TerminalResumePoint(),
      options: terminalOptions(),
    });

    // When two keystrokes are typed
    feed.stream.send(bytesOf("l"));
    feed.stream.send(bytesOf("s\r"));

    // Then each states the running total the daemon acks against
    expect(daemon.sends.map((send) => send.inputOffset)).toEqual([1n, 3n]);
  });

  it("fetches scrollback from the daemon that holds the capture ring", async () => {
    // Given a feed on a daemon holding retained output
    const daemon = aDaemonReplaying([]);
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
    const daemon = aDaemonReplaying([]);
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
    const daemon = aDaemonReplaying([
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
