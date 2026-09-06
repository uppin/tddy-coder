/**
 * Unit tests for the terminal feed a room-carried session offers.
 *
 * The design these pin has two halves that must not be confused, because confusing them is the
 * bug this node exists to prevent:
 *
 *   • **Bytes come off the room.** `StreamTerminalIO` against the participant the session's own
 *     process serves on — the only method that participant answers.
 *   • **Scrollback comes off the host daemon.** A session room serves the PTY and nothing else, so
 *     history asked of the room would be asked of something that cannot answer it. The capture ring
 *     is the host's, and asking the host is what gives a LiveKit-carried session the scrollback it
 *     has never had.
 *
 * The two clients are therefore deliberately distinguishable here: the session participant answers
 * `GetTerminalHistory` too, with an answer of its own, so a feed wired to the wrong one is caught
 * rather than passing on a shape that happens to match.
 *
 * Technical: `packages/tddy-web/src/rpc/connections/livekit/roomTerminalFeed.ts`
 */

import { describe, expect, it } from "bun:test";
import { create } from "@bufbuild/protobuf";
import type { Client } from "@connectrpc/connect";
import { RoomEvent, type Room } from "livekit-client";
import { type ConnectionService, TerminalHistoryChunkSchema } from "../../../gen/connection_pb";
import { TerminalOutputSchema, type TerminalService } from "../../../gen/terminal_pb";
import type { HistoryChunk } from "../../../lib/terminalHistoryLoader";
import type { TerminalFeed, TerminalHistoryFetcher, TerminalOptions } from "../terminal";
import { openRoomTerminalFeed } from "./roomTerminalFeed";

const A_SESSION = "session-0001";
const A_SERVER_IDENTITY = "daemon-instance-a-session-0001";
const ANOTHER_PARTICIPANT = "browser-bob";
const A_SESSION_TOKEN = "gho-alice";
const A_TERMINAL_ID = "bash-2";

/** What the host's capture ring holds for this terminal, and only the host's. */
const HOST_SCROLLBACK = "cargo build --release\r\n";

/**
 * What the session participant would answer if the feed asked *it* for history.
 *
 * Deliberately different from {@link HOST_SCROLLBACK}: a double that answered the same thing could
 * not tell a feed built against the host from one built against the room.
 */
const ROOM_SCROLLBACK = "the room does not keep scrollback";

/**
 * Three drain intervals' worth of quiet — `INPUT_DRAIN_INTERVAL_MS` is 20ms.
 *
 * The only place these specs wait on a duration, and only for the assertions about something *not*
 * happening: there is no event to synchronise on when the feed is supposed to stay silent.
 */
const aQuietMoment = (): Promise<void> => new Promise((resolve) => setTimeout(resolve, 60));

const bytesOf = (text: string): Uint8Array => new TextEncoder().encode(text);
const textOf = (bytes: Uint8Array): string => new TextDecoder().decode(bytes);

/** The text of a fetched chunk — these specs always fetch one that exists. */
function textOfChunk(chunk: HistoryChunk | null): string {
  if (chunk === null) throw new Error("expected the fetcher to return a chunk");
  return textOf(chunk.data);
}

/**
 * Whether the feed has reported its remote end, decided within {@link aQuietMoment}.
 *
 * Raced rather than awaited so a feed that never settles fails as an assertion naming what it did
 * instead, rather than as a test that timed out.
 */
async function endStateOf(feed: TerminalFeed): Promise<"ended" | "still live"> {
  const { ended } = feed;
  if (ended === undefined) throw new Error("expected the feed to report its remote end");
  return await Promise.race([
    ended.then(() => "ended" as const),
    aQuietMoment().then(() => "still live" as const),
  ]);
}

/** A room-carried feed always offers scrollback, because its host can always serve it. */
function scrollbackOf(feed: TerminalFeed): TerminalHistoryFetcher {
  const { history } = feed;
  if (history === undefined) throw new Error("expected the feed to offer a history fetcher");
  return history;
}

/** What a pane states when it opens this session's bash terminal. */
function terminalOptions(): TerminalOptions {
  return {
    terminalId: A_TERMINAL_ID,
    sessionToken: A_SESSION_TOKEN,
    // The room membership is the authorisation on this wire, so the feed never reads this.
    controlToken: () => "lease-held-elsewhere",
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

/** The room the session publishes into, with a roster a test can admit participants onto. */
function aRoomHolding(participants: string[]) {
  const handlers = new Map<string, Array<(arg: { identity: string }) => void>>();
  const remoteParticipants = new Map(participants.map((identity) => [identity, { identity }]));
  const room = {
    remoteParticipants,
    on(event: string, fn: (arg: { identity: string }) => void) {
      handlers.set(event, [...(handlers.get(event) ?? []), fn]);
      return room;
    },
    off(event: string, fn: (arg: { identity: string }) => void) {
      handlers.set(event, (handlers.get(event) ?? []).filter((registered) => registered !== fn));
      return room;
    },
  };
  return {
    room: room as unknown as Room,
    admit(identity: string): void {
      remoteParticipants.set(identity, { identity });
      for (const fn of handlers.get(RoomEvent.ParticipantConnected) ?? []) fn({ identity });
    },
    evict(identity: string): void {
      remoteParticipants.delete(identity);
      for (const fn of handlers.get(RoomEvent.ParticipantDisconnected) ?? []) fn({ identity });
    },
    joinWatchers: (): number => (handlers.get(RoomEvent.ParticipantConnected) ?? []).length,
  };
}

/**
 * The participant the session's own process serves on: a bidi `StreamTerminalIO` whose output side
 * a test drives, and which records every input message it is handed.
 *
 * It also answers `GetTerminalHistory` — not because the real one does, but so a feed that asked
 * the room for scrollback would be seen doing it.
 */
function aSessionParticipant() {
  const outbound: ReturnType<typeof create<typeof TerminalOutputSchema>>[] = [];
  const typedBatches: Uint8Array[] = [];
  const historyRequests: HistoryRequest[] = [];
  let done = false;
  let wake: () => void = () => {};
  let awake = new Promise<void>((resolve) => (wake = resolve));
  let announceTyped: (batch: Uint8Array) => void = () => {};
  const firstTypedBatch = new Promise<Uint8Array>((resolve) => (announceTyped = resolve));
  const bump = () => {
    wake();
    awake = new Promise<void>((resolve) => (wake = resolve));
  };

  async function collectInput(input: AsyncIterable<{ data: Uint8Array }>): Promise<void> {
    for await (const message of input) {
      // The bridge's opening frame is empty and carries nothing a test typed.
      if (message.data.length === 0) continue;
      typedBatches.push(message.data);
      announceTyped(message.data);
    }
  }

  const client = {
    async *streamTerminalIO(input: AsyncIterable<{ data: Uint8Array }>) {
      opened = true;
      announceOpen();
      void collectInput(input);
      while (true) {
        while (outbound.length > 0) yield outbound.shift() as (typeof outbound)[number];
        if (done) return;
        await awake;
      }
    },
    async *getTerminalHistory(req: HistoryRequest) {
      historyRequests.push(req);
      yield create(TerminalHistoryChunkSchema, { data: bytesOf(ROOM_SCROLLBACK), atEnd: true });
    },
  };

  let opened = false;
  let announceOpen: () => void = () => {};
  const streamOpened = new Promise<void>((resolve) => (announceOpen = resolve));

  return {
    client: client as unknown as Client<typeof TerminalService>,
    emit(text: string): void {
      outbound.push(create(TerminalOutputSchema, { data: bytesOf(text) }));
      bump();
    },
    endStream(): void {
      done = true;
      bump();
    },
    streamOpened,
    hasOpenedStream: (): boolean => opened,
    typedBatches,
    firstTypedBatch,
    historyRequests,
  };
}

/** The host daemon, which holds the capture ring and is the only thing that can replay it. */
function aHostDaemon() {
  const historyRequests: HistoryRequest[] = [];
  const client = {
    async *getTerminalHistory(req: HistoryRequest) {
      historyRequests.push(req);
      yield create(TerminalHistoryChunkSchema, {
        data: bytesOf(HOST_SCROLLBACK),
        startOffset: 0n,
        endOffset: BigInt(HOST_SCROLLBACK.length),
        atOldest: true,
        atEnd: true,
      });
    },
  };
  return { client: client as unknown as Client<typeof ConnectionService>, historyRequests };
}

/** A feed on a room the session's process has already joined. */
function aFeedOnALiveRoom() {
  const roster = aRoomHolding([A_SERVER_IDENTITY, ANOTHER_PARTICIPANT]);
  const participant = aSessionParticipant();
  const host = aHostDaemon();
  const feed = openRoomTerminalFeed({
    room: roster.room,
    serverIdentity: A_SERVER_IDENTITY,
    terminal: participant.client,
    host: host.client,
    sessionId: A_SESSION,
    options: terminalOptions(),
  });
  return { feed, roster, participant, host };
}

describe("a room-carried terminal feed", () => {
  it("delivers the bytes the session participant streams to its listener", async () => {
    // Given a feed on a room whose session process is already there
    const { feed, participant } = aFeedOnALiveRoom();
    const painted: string[] = [];
    feed.stream.onMessage((frame) => painted.push(textOf(frame.data)));

    // When the participant streams two chunks of output and closes the stream
    participant.emit("cargo test\r\n");
    participant.emit("running 12 tests\r\n");
    participant.endStream();
    await feed.ended;

    // Then both reached the terminal, in the order the room carried them
    expect(painted).toEqual(["cargo test\r\n", "running 12 tests\r\n"]);
  });

  it("reports every frame as an unanchored live tail, because the room's wire carries no offsets", async () => {
    // Given a feed on a live room. `terminal.TerminalOutput` is `bytes data` and nothing else —
    // there is no offset on this wire to report, which is why a fill it drives is unanchored.
    const { feed, participant } = aFeedOnALiveRoom();
    const frames: unknown[] = [];
    feed.stream.onMessage((frame) => frames.push(frame));

    // When one chunk of output arrives
    participant.emit("$ ");
    participant.endStream();
    await feed.ended;

    // Then it carries bytes and no anchor at all
    expect(frames).toEqual([{ data: bytesOf("$ "), endOffset: 0n, atOldest: false }]);
  });

  it("fetches scrollback from the host daemon rather than from the room", async () => {
    // Given a feed whose host holds the capture ring, and a participant that would answer with
    // something else entirely if it were asked
    const { feed } = aFeedOnALiveRoom();

    // When the terminal pages backwards
    const chunk = await scrollbackOf(feed)(0n, 0n);

    // Then it is reading the host's capture ring — the scrollback a room-carried session has never
    // had, and which the room itself cannot serve
    expect(textOfChunk(chunk)).toEqual(HOST_SCROLLBACK);
  });

  it("asks the host for this session's own terminal, from the start of retained history", async () => {
    // Given a feed for the session's bash terminal
    const { feed, host } = aFeedOnALiveRoom();

    // When the terminal pages backwards
    await scrollbackOf(feed)(0n, 0n);

    // Then the host was asked for exactly that terminal, authorised by the caller's session token
    expect(host.historyRequests).toEqual([
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

  it("never asks the session participant for scrollback", async () => {
    // Given a feed on a room that serves the PTY and nothing else
    const { feed, participant } = aFeedOnALiveRoom();

    // When the terminal pages backwards
    await scrollbackOf(feed)(0n, 0n);

    // Then nothing was asked of the room: history is not on this wire
    expect(participant.historyRequests).toEqual([]);
  });

  it("drains everything typed between ticks into a single message", async () => {
    // Given a feed on a live room. One data-channel message per keystroke is what overflowed the
    // send buffer on a paste, so a tick's worth of typing travels as one message.
    const { feed, participant } = aFeedOnALiveRoom();

    // When three keystrokes land inside one drain interval
    feed.stream.send(bytesOf("l"));
    feed.stream.send(bytesOf("s"));
    feed.stream.send(bytesOf("\r"));

    // Then the participant received them as one message
    expect(textOf(await participant.firstTypedBatch)).toEqual("ls\r");
  });

  it("sends nothing more once the feed is closed", async () => {
    // Given a feed that has already carried a keystroke to the session's PTY
    const { feed, participant } = aFeedOnALiveRoom();
    feed.stream.send(bytesOf("echo hi"));
    await participant.firstTypedBatch;

    // When the pane goes away and something is typed into it afterwards
    feed.stream.close();
    feed.stream.send(bytesOf("rm -rf /"));
    await aQuietMoment();

    // Then only what was typed while it was open ever reached the PTY
    expect(participant.typedBatches.map(textOf)).toEqual(["echo hi"]);
  });

  it("reports the session ended when its process leaves the room", async () => {
    // Given a feed on a live room. A participant that vanishes takes a while to surface as a stream
    // error, and the roster says so at once — which is what covers the pane promptly enough to stop
    // the operator typing into a terminal nobody is reading.
    const { feed, roster } = aFeedOnALiveRoom();

    // When the session's process leaves
    roster.evict(A_SERVER_IDENTITY);

    // Then the pane is told, without waiting for the stream to notice
    expect(await endStateOf(feed)).toEqual("ended");
  });

  it("keeps tailing when some other participant leaves the room", async () => {
    // Given a feed on a room with a second browser on it
    const { feed, participant, roster } = aFeedOnALiveRoom();
    const painted: string[] = [];
    feed.stream.onMessage((frame) => painted.push(textOf(frame.data)));

    // When that other participant leaves and the session keeps producing output
    roster.evict(ANOTHER_PARTICIPANT);
    participant.emit("still here\r\n");
    participant.endStream();
    await feed.ended;

    // Then the terminal went on painting — someone else's departure is not this session's end
    expect(painted).toEqual(["still here\r\n"]);
  });

  it("does not open the stream until the session's process is on the roster", async () => {
    // Given a room the session's process has not joined yet — addressing a participant that is not
    // there fails outright rather than waiting
    const roster = aRoomHolding([ANOTHER_PARTICIPANT]);
    const participant = aSessionParticipant();

    // When a feed is opened on it
    openRoomTerminalFeed({
      room: roster.room,
      serverIdentity: A_SERVER_IDENTITY,
      terminal: participant.client,
      host: aHostDaemon().client,
      sessionId: A_SESSION,
      options: terminalOptions(),
    });
    await aQuietMoment();

    // Then no stream was opened against it
    expect(participant.hasOpenedStream()).toBe(false);
  });

  it("opens the stream once the session's process joins the room", async () => {
    // Given a feed waiting on a room the session's process has not joined yet
    const roster = aRoomHolding([ANOTHER_PARTICIPANT]);
    const participant = aSessionParticipant();
    openRoomTerminalFeed({
      room: roster.room,
      serverIdentity: A_SERVER_IDENTITY,
      terminal: participant.client,
      host: aHostDaemon().client,
      sessionId: A_SESSION,
      options: terminalOptions(),
    });

    // When the session's process arrives
    roster.admit(A_SERVER_IDENTITY);
    await participant.streamOpened;

    // Then the terminal is open against it
    expect(participant.hasOpenedStream()).toBe(true);
  });

  it("stops watching the roster when it is closed while still waiting", async () => {
    // Given a feed still waiting for a session that is coming up
    const roster = aRoomHolding([ANOTHER_PARTICIPANT]);
    const feed = openRoomTerminalFeed({
      room: roster.room,
      serverIdentity: A_SERVER_IDENTITY,
      terminal: aSessionParticipant().client,
      host: aHostDaemon().client,
      sessionId: A_SESSION,
      options: terminalOptions(),
    });

    // When the pane goes away before the session ever arrives
    feed.stream.close();

    // Then nothing of it is left on the room — an abandoned wait would otherwise hold a
    // ParticipantConnected handler for the life of the page
    expect(roster.joinWatchers()).toEqual(0);
  });
});
