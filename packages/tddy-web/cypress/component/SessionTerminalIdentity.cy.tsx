/**
 * Behaviour spec: the participant identity a session's terminal is reached under.
 *
 * The terminal used to perform a **second**, independent join of the session's room — separate from
 * the one the session's RPC travelled over — minting an identity of its own and having a browser
 * token issued for it. It no longer does: it reads its bytes off the session's connection, and that
 * connection's join is the only one. The identity went with it, and so did this spec.
 *
 * Two things about that identity are load-bearing, and both fail silently when they are wrong,
 * because LiveKit's answer to a duplicate identity is to drop one of the two participants rather
 * than to complain. It must hold for the life of the attachment — a regenerated identity is a fresh
 * join under a new participant, with the old one still on the roster — and it must not be
 * reproducible, because two joins that overlap (a remount before the previous participant is
 * reaped, two tabs on one session) would otherwise mint the same string.
 *
 * The mint here never answers, so nothing connects and no media server is involved: what the
 * connection *asks for* is the whole of the behaviour under test.
 *
 * Technical: `packages/tddy-web/docs/session-connections.md`.
 */

import React from "react";
import type { Client, Transport } from "@connectrpc/connect";
import type { DescService } from "@bufbuild/protobuf";
import { ConnectionState, type Room } from "livekit-client";
import type { TokenService } from "../../src/gen/token_pb";
import {
  openLiveKitSession,
  type LiveKitSessionSupport,
} from "../../src/rpc/connections/livekit/sessionConnection";
import type { SessionConnection } from "../../src/rpc/connections/session";
import { byTestId } from "../support/testIds";

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

const A_HOST = "dev";
const A_SESSION = "session-0001";
const A_ROOM = "daemon-session-0001";
const ANOTHER_ROOM = "daemon-session-0001-resumed";
const NOWHERE = "ws://127.0.0.1:9999";

const RE_RENDER_BTN = "re-render";
const CHANGE_ROOM_BTN = "change-room";
const MINTS_EL = "token-mints";

/**
 * A mint that records what it was asked for and never answers.
 *
 * Never answering is deliberate: the join waits on the token, so no room is ever connected and
 * nothing here depends on a media server.
 */
function aRecordingTokenMint() {
  const requests: { room: string; identity: string }[] = [];
  const client = {
    generateToken: (req: { room: string; identity: string }) => {
      requests.push(req);
      return new Promise(() => {});
    },
    refreshToken: () => new Promise(() => {}),
  } as unknown as Client<typeof TokenService>;
  return { client, requests };
}

/** A room that is never connected — the join never gets past the mint. */
function anUnjoinedRoom(): Room {
  return {
    state: ConnectionState.Disconnected,
    remoteParticipants: new Map(),
    on: () => {},
    off: () => {},
    connect: async () => {},
    disconnect: () => {},
  } as unknown as Room;
}

function supportedBy(tokens: Client<typeof TokenService>): LiveKitSessionSupport {
  return {
    tokens,
    // Neither is reached: nothing in these specs issues a call or opens a terminal, and a stub that
    // answered one would be claiming a wire that is not there.
    transportFor: () => {
      throw new Error("this session's routing is not under test");
    },
    hostClientFor: <S extends DescService>(): Client<S> => {
      throw new Error("this session's host is not under test");
    },
    newRoom: anUnjoinedRoom,
  } as LiveKitSessionSupport;
}

/** Open `room`'s session the way an attachment does — once, and held for its lifetime. */
function useAttachedSession(
  tokens: Client<typeof TokenService>,
  room: string,
): SessionConnection {
  const [held, setHeld] = React.useState(() => ({
    room,
    connection: openLiveKitSession(A_HOST, { sessionId: A_SESSION, room, url: NOWHERE }, supportedBy(tokens)),
  }));
  if (held.room !== room) {
    held.connection.close();
    setHeld({
      room,
      connection: openLiveKitSession(A_HOST, { sessionId: A_SESSION, room, url: NOWHERE }, supportedBy(tokens)),
    });
  }
  return held.connection;
}

// ---------------------------------------------------------------------------
// Harness
// ---------------------------------------------------------------------------

/** One attached session, whose room the test can change and whose pane the test can re-render. */
function SessionHarness({ tokens }: { tokens: Client<typeof TokenService> }) {
  const [room, setRoom] = React.useState(A_ROOM);
  const [renders, setRenders] = React.useState(1);
  const connection = useAttachedSession(tokens, room);

  return (
    <div>
      <button data-testid={RE_RENDER_BTN} onClick={() => setRenders((n) => n + 1)}>
        re-render
      </button>
      <button data-testid={CHANGE_ROOM_BTN} onClick={() => setRoom(ANOTHER_ROOM)}>
        change room
      </button>
      <div data-testid="renders">{renders}</div>
      <div data-testid="session">{connection.sessionId}</div>
    </div>
  );
}

/** Two attachments to the same session's room at once — two tabs, or a racing remount. */
function TwoSessionsHarness({ tokens }: { tokens: Client<typeof TokenService> }) {
  const first = useAttachedSession(tokens, A_ROOM);
  const second = useAttachedSession(tokens, A_ROOM);
  return (
    <div data-testid={MINTS_EL}>
      {first.sessionId}/{second.sessionId}
    </div>
  );
}

// ---------------------------------------------------------------------------
// Specs
// ---------------------------------------------------------------------------

describe("a session terminal's own participant identity", () => {
  it("holds across re-renders, so the join is not repeated under a new participant", () => {
    // Given an attached session that has asked for its token
    const mint = aRecordingTokenMint();
    cy.mount(<SessionHarness tokens={mint.client} />);
    cy.wrap(null).should(() => expect(mint.requests).to.have.length(1));

    // When the pane re-renders twice — every keystroke in the drawer does this
    byTestId(RE_RENDER_BTN).click();
    byTestId(RE_RENDER_BTN).click();
    byTestId("renders").should("have.text", "3");

    // Then no second token was minted. A fresh identity per render would leave the room holding a
    // trail of abandoned participants, each still counted against the session
    cy.wrap(null).should(() => expect(mint.requests).to.have.length(1));
  });

  it("is minted afresh for a different room", () => {
    // Given a session attached over one room
    const mint = aRecordingTokenMint();
    cy.mount(<SessionHarness tokens={mint.client} />);
    cy.wrap(null).should(() => expect(mint.requests).to.have.length(1));

    // When the session is re-attached into a different room
    byTestId(CHANGE_ROOM_BTN).click();

    // Then a second, different identity joins it. Two rooms are two participants, and a token
    // issued for the first room is not accepted by the second
    cy.wrap(null).should(() => {
      expect(mint.requests.map((r) => r.room)).to.deep.equal([A_ROOM, ANOTHER_ROOM]);
      expect(mint.requests[1].identity).to.not.equal(mint.requests[0].identity);
    });
  });

  it("differs between two attachments watching the same room", () => {
    // Given two attachments to one session's room, mounted together
    const mint = aRecordingTokenMint();

    cy.mount(<TwoSessionsHarness tokens={mint.client} />);

    // Then they join as two participants. Minting from the clock alone puts both in the same
    // millisecond, and a room that sees an identity twice silently drops one of the two
    byTestId(MINTS_EL).should("exist");
    cy.wrap(null).should(() => {
      expect(mint.requests).to.have.length(2);
      expect(mint.requests[1].identity).to.not.equal(mint.requests[0].identity);
    });
  });
});
