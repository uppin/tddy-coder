/**
 * The terminal feed a room-carried session offers.
 *
 * Bytes travel the way `GhosttyTerminalLiveKit` already sent them: a bidi
 * `terminal.TerminalService/StreamTerminalIO` against the participant the session's own process
 * serves on, which is the only method that participant answers (`cli_session_manager.rs` —
 * `PtyLiveKitService`). Nothing about that routing changes here; what changes is that the component
 * no longer performs it.
 *
 * **History does not travel that way, and this is the point of the node.** A session room serves
 * the PTY and nothing else, so `GhosttyTerminalLiveKit` had no history fetcher, no offset tracking
 * and no page terminal — a LiveKit session could not see past what was live. The capture ring is
 * the *host daemon's*, and the host is reachable: asking it for `GetTerminalHistory` gives a
 * room-carried session the scrollback it has never had, without moving a single output byte off the
 * room.
 */

import { create } from "@bufbuild/protobuf";
import type { Client } from "@connectrpc/connect";
import { RoomEvent, type Room } from "livekit-client";
import type { ConnectionService } from "../../../gen/connection_pb";
import { TerminalInputSchema, type TerminalService } from "../../../gen/terminal_pb";
import { tddyDebug } from "../../../lib/debugMask";
import { createForwardHistoryFetcher } from "../../../lib/terminalHistoryLoader";
import type { TerminalFeed, TerminalFrame, TerminalOptions, TerminalStream } from "../terminal";

const dTerm = tddyDebug("tddy:term:feed");

/**
 * How long the input generator waits before looking at the outbound queue again.
 *
 * `GhosttyTerminalLiveKit`'s interval, kept: an async generator has nothing to await when the queue
 * is empty, and each yield is one WebRTC data-channel message. Draining the whole queue into a
 * single message per tick is what stopped rapid typing and pastes overflowing the channel's send
 * buffer — a thousand one-byte sends became a handful of large ones, and the drops that produced
 * went away with them.
 */
const INPUT_DRAIN_INTERVAL_MS = 20;

export interface RoomTerminalFeedDeps {
  /** The session's own room, joined by the connection that owns this feed. */
  readonly room: Room;

  /** The participant the session's process serves `StreamTerminalIO` on. */
  readonly serverIdentity: string;

  /** Addressed at {@link serverIdentity} over {@link room}. */
  readonly terminal: Client<typeof TerminalService>;

  /**
   * The **host daemon's** client, not the session participant's.
   *
   * Scrollback lives in the host's capture ring; the room cannot serve it. This is the whole of
   * what makes history available on a LiveKit-carried session.
   */
  readonly host: Client<typeof ConnectionService>;

  readonly sessionId: string;
  readonly options: TerminalOptions;
}

/** Concatenate the queued chunks into one message's worth of bytes. */
function drained(chunks: Uint8Array[]): Uint8Array {
  const total = chunks.reduce((sum, chunk) => sum + chunk.length, 0);
  const batch = new Uint8Array(total);
  let at = 0;
  for (const chunk of chunks) {
    batch.set(chunk, at);
    at += chunk.length;
  }
  return batch;
}

/**
 * Settles once `identity` is on `room`'s roster, and can be abandoned.
 *
 * Opening the stream before the session's process has joined addresses a participant that is not
 * there, and the call fails outright rather than waiting. The abandon path matters as much as the
 * wait: a feed closed while its session was still coming up would otherwise leave a
 * `ParticipantConnected` handler on a room nobody is watching, for the life of the page.
 */
function whenServerJoins(room: Room, identity: string): { joined: Promise<void>; abandon: () => void } {
  const present = () =>
    Array.from(room.remoteParticipants.values()).some((peer) => peer.identity === identity);

  let abandon = () => {};
  const joined = new Promise<void>((resolve) => {
    if (present()) {
      resolve();
      return;
    }
    const onParticipant = () => {
      if (!present()) return;
      room.off(RoomEvent.ParticipantConnected, onParticipant);
      resolve();
    };
    room.on(RoomEvent.ParticipantConnected, onParticipant);
    abandon = () => {
      room.off(RoomEvent.ParticipantConnected, onParticipant);
      resolve();
    };
  });

  return { joined, abandon: () => abandon() };
}

/**
 * Open `sessionId`'s terminal over the room its process publishes into.
 *
 * `options.controlToken` is not read: `StreamTerminalIO` writes straight to the PTY handle the
 * session's own process holds, so there is no lease for the daemon to compare against — the room
 * membership *is* the authorisation. `options.initialGrid` is likewise unused; this wire has no
 * pre-replay resize, and the terminal states its size with the resize OSC the PTY bridge parses out
 * of the input stream.
 */
export function openRoomTerminalFeed({
  room,
  serverIdentity,
  terminal,
  host,
  sessionId,
  options,
}: RoomTerminalFeedDeps): TerminalFeed {
  const terminalId = options.terminalId ?? "";
  const listeners: Array<(frame: TerminalFrame) => void> = [];
  const outbound: Uint8Array[] = [];
  let closed = false;

  const waitingForServer = whenServerJoins(room, serverIdentity);

  const stream: TerminalStream = {
    send(data: Uint8Array): void {
      if (closed) return;
      outbound.push(data);
    },
    onMessage(fn: (frame: TerminalFrame) => void): void {
      listeners.push(fn);
    },
    close(): void {
      closed = true;
      waitingForServer.abandon();
    },
  };

  async function* input(): AsyncGenerator<ReturnType<typeof create<typeof TerminalInputSchema>>> {
    // The bridge starts its PTY read side on the first message of the stream, so one empty frame
    // opens the terminal before anything has been typed into it.
    yield create(TerminalInputSchema, { data: new Uint8Array(0) });
    while (!closed) {
      if (outbound.length === 0) {
        await new Promise((resume) => setTimeout(resume, INPUT_DRAIN_INTERVAL_MS));
        continue;
      }
      yield create(TerminalInputSchema, { data: drained(outbound.splice(0)) });
    }
  }

  void (async () => {
    try {
      await waitingForServer.joined;
      if (closed) return;
      for await (const output of terminal.streamTerminalIO(input())) {
        if (closed) break;
        if (output.data.length === 0) continue;
        // `terminal.TerminalOutput` is `bytes data` and nothing else, so every frame is a live tail
        // frame: there is no anchor on this wire, which is why a fill driven by this feed's history
        // fetcher is constructed unanchored.
        const frame: TerminalFrame = { data: output.data, endOffset: 0n, atOldest: false };
        for (const fn of listeners) fn(frame);
      }
    } catch (err) {
      dTerm(
        "streamTerminalIO ended sessionId=%s serverIdentity=%s error=%o",
        sessionId,
        serverIdentity,
        err instanceof Error ? err.message : err,
      );
    }
  })();

  return {
    stream,
    history: createForwardHistoryFetcher(host, {
      sessionToken: options.sessionToken,
      sessionId,
      terminalId,
    }),
  };
}
