/**
 * Behaviour spec: the participant identity a session's terminal joins its room under.
 *
 * `SessionLiveKitTerminal` still performs a **second**, independent join of the session's room —
 * separate from the one the session's RPC travels over — so it mints an identity of its own and has
 * a browser token issued for it. Two things about that identity are load-bearing, and both fail
 * silently when they are wrong, because LiveKit's answer to a duplicate identity is to drop one of
 * the two participants rather than to complain.
 *
 * It must hold across re-renders — a regenerated identity is a fresh join under a new participant,
 * with the old one still on the roster — and it must not be reproducible, because two joins that
 * overlap (a remount before the previous participant is reaped, two tabs on one session) would
 * otherwise mint the same string.
 *
 * The token mint here never answers, so `GhosttyTerminalLiveKit` is never reached: what the terminal
 * *asks for* is the whole of the behaviour under test.
 *
 * TODO(optional-livekit node 5): folds into the session connection's own join, and this identity
 * goes with it.
 *
 * Technical: `packages/tddy-web/docs/session-connections.md`.
 */

import React from "react";
import type { Client } from "@connectrpc/connect";
import type { TokenService } from "../../src/gen/token_pb";
import { SessionLiveKitTerminal } from "../../src/components/sessions/SessionLiveKitTerminal";
import { byTestId } from "../support/testIds";

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

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
 * Never answering is deliberate: the terminal renders nothing until a token arrives, so the real
 * LiveKit terminal is never mounted and nothing here depends on a media server.
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

// ---------------------------------------------------------------------------
// Harness
// ---------------------------------------------------------------------------

/** One terminal, whose room the test can change and whose parent the test can re-render. */
function TerminalHarness({ tokenClient }: { tokenClient: Client<typeof TokenService> }) {
  const [room, setRoom] = React.useState(A_ROOM);
  const [renders, setRenders] = React.useState(1);

  return (
    <div>
      <button data-testid={RE_RENDER_BTN} onClick={() => setRenders((n) => n + 1)}>
        re-render
      </button>
      <button data-testid={CHANGE_ROOM_BTN} onClick={() => setRoom(ANOTHER_ROOM)}>
        change room
      </button>
      <div data-testid="renders">{renders}</div>
      <SessionLiveKitTerminal
        livekitUrl={NOWHERE}
        livekitRoom={room}
        livekitServerIdentity={`daemon-${A_SESSION}`}
        tokenClient={tokenClient}
        sessionToken="an-operator-access-token"
        sessionId={A_SESSION}
      />
    </div>
  );
}

/** Two terminals watching the same session's room at once — two tabs, or a racing remount. */
function TwoTerminalsHarness({ tokenClient }: { tokenClient: Client<typeof TokenService> }) {
  const terminal = (key: string) => (
    <SessionLiveKitTerminal
      key={key}
      livekitUrl={NOWHERE}
      livekitRoom={A_ROOM}
      livekitServerIdentity={`daemon-${A_SESSION}`}
      tokenClient={tokenClient}
      sessionToken="an-operator-access-token"
      sessionId={A_SESSION}
    />
  );
  return (
    <div>
      {terminal("first")}
      {terminal("second")}
      <div data-testid={MINTS_EL}>mounted</div>
    </div>
  );
}

// ---------------------------------------------------------------------------
// Specs
// ---------------------------------------------------------------------------

describe("a session terminal's own participant identity", () => {
  it("holds across re-renders, so the join is not repeated under a new participant", () => {
    // Given a mounted terminal that has asked for its token
    const mint = aRecordingTokenMint();
    cy.mount(<TerminalHarness tokenClient={mint.client} />);
    cy.wrap(null).should(() => expect(mint.requests).to.have.length(1));

    // When its parent re-renders twice — every keystroke in the drawer does this
    byTestId(RE_RENDER_BTN).click();
    byTestId(RE_RENDER_BTN).click();
    byTestId("renders").should("have.text", "3");

    // Then no second token was minted. A fresh identity per render would leave the room holding a
    // trail of abandoned participants, each still counted against the session
    cy.wrap(null).should(() => expect(mint.requests).to.have.length(1));
  });

  it("is minted afresh for a different room", () => {
    // Given a terminal watching one room
    const mint = aRecordingTokenMint();
    cy.mount(<TerminalHarness tokenClient={mint.client} />);
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

  it("differs between two terminals watching the same room", () => {
    // Given two terminals on one session's room, mounted together
    const mint = aRecordingTokenMint();

    cy.mount(<TwoTerminalsHarness tokenClient={mint.client} />);

    // Then they join as two participants. Minting from the clock alone puts both in the same
    // millisecond, and a room that sees an identity twice silently drops one of the two
    byTestId(MINTS_EL).should("exist");
    cy.wrap(null).should(() => {
      expect(mint.requests).to.have.length(2);
      expect(mint.requests[1].identity).to.not.equal(mint.requests[0].identity);
    });
  });
});
