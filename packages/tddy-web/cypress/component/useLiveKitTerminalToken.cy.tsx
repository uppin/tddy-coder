/**
 * Behaviour spec: `useLiveKitTerminalToken` — the shared browser-token hook that
 * lets any LiveKit-backed session terminal (tddy-coder recipe sessions today,
 * remotely-routed Claude CLI sessions tomorrow) fetch and refresh a LiveKit
 * access token the same way `ConnectionScreen.tsx`'s `ConnectedTerminal` already
 * does inline.
 *
 * Changeset: unify tddy-coder recipe-session terminals onto the same LiveKit
 * terminal component already used for Claude CLI (`GhosttyTerminalLiveKit`).
 *
 * This module does not exist yet — every test below fails at bundle time
 * (module not found) until `useLiveKitTerminalToken.ts` is added under
 * `packages/tddy-web/src/components/sessions/`.
 */

import React, { useMemo, useState } from "react";
import { createClient } from "@connectrpc/connect";
import { createConnectTransport } from "@connectrpc/connect-web";
import { create, fromBinary, toBinary } from "@bufbuild/protobuf";
import {
  TokenService,
  GenerateTokenRequestSchema,
  GenerateTokenResponseSchema,
  RefreshTokenRequestSchema,
  RefreshTokenResponseSchema,
} from "../../src/gen/token_pb";
import { useLiveKitTerminalToken } from "../../src/components/sessions/useLiveKitTerminalToken";
import { useHttpClient } from "../../src/rpc/transportProvider";
import { decodeProtoRequestBody, toArrayBuffer } from "../support/rpc/protoRpc";
import { withSessionTokenGate } from "../support/rpc/withSessionTokenGate";
import { byTestId } from "../support/testIds";

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

const ROOM_NAME = "room-token-hook-test-0001";
const IDENTITY = "browser-token-hook-test-0001-999";
/** The signed-in operator's daemon access token — what the daemon's mint refuses to act without. */
const SESSION_TOKEN = "an-operator-access-token";

const GENERATE_OK = toArrayBuffer(
  toBinary(
    GenerateTokenResponseSchema,
    create(GenerateTokenResponseSchema, { token: "lk-initial-token", ttlSeconds: BigInt(600) }),
  ),
);

const REFRESH_OK = toArrayBuffer(
  toBinary(
    RefreshTokenResponseSchema,
    create(RefreshTokenResponseSchema, { token: "lk-refreshed-token", ttlSeconds: BigInt(120) }),
  ),
);

// ---------------------------------------------------------------------------
// Harness
// ---------------------------------------------------------------------------

const TOKEN_EL = "lk-token-hook-token";
const TTL_EL = "lk-token-hook-ttl";
const ERROR_EL = "lk-token-hook-error";
const REFRESH_BTN = "lk-token-hook-refresh-btn";
const REFRESHED_TOKEN_EL = "lk-token-hook-refreshed-token";

function TokenHookHarness({ room, identity }: { room?: string; identity?: string }) {
  const transport = useMemo(
    () => createConnectTransport({ baseUrl: `${window.location.origin}/rpc`, useBinaryFormat: true }),
    [],
  );
  const tokenClient = useMemo(() => createClient(TokenService, transport), [transport]);
  const { token, ttlSeconds, error, getToken } = useLiveKitTerminalToken(tokenClient, room, identity);
  const [refreshedToken, setRefreshedToken] = useState("");

  return (
    <div>
      <span data-testid={TOKEN_EL}>{token ?? ""}</span>
      <span data-testid={TTL_EL}>{ttlSeconds === null ? "" : ttlSeconds.toString()}</span>
      <span data-testid={ERROR_EL}>{error ?? ""}</span>
      <span data-testid={REFRESHED_TOKEN_EL}>{refreshedToken}</span>
      <button
        type="button"
        data-testid={REFRESH_BTN}
        onClick={() => void getToken().then((res) => setRefreshedToken(res.token))}
      >
        refresh
      </button>
    </div>
  );
}

/**
 * The same harness on the *production* transport (`useHttpClient`) rather than a bare
 * `createConnectTransport`, so requests pass through the auth gate the app installs.
 */
function GatedTokenHookHarness({ room, identity }: { room?: string; identity?: string }) {
  const tokenClient = useHttpClient(TokenService);
  const { token, getToken } = useLiveKitTerminalToken(tokenClient, room, identity);

  return (
    <div>
      <span data-testid={TOKEN_EL}>{token ?? ""}</span>
      <button
        type="button"
        data-testid={REFRESH_BTN}
        onClick={() => void getToken().then(() => undefined)}
      >
        refresh
      </button>
    </div>
  );
}

function interceptGenerateToken(body: ArrayBuffer, alias = "generateToken") {
  cy.intercept("POST", "**/rpc/token.TokenService/GenerateToken", (req) => {
    req.reply({ statusCode: 200, headers: { "Content-Type": "application/proto" }, body });
  }).as(alias);
}

function interceptRefreshToken(body: ArrayBuffer, alias = "refreshToken") {
  cy.intercept("POST", "**/rpc/token.TokenService/RefreshToken", (req) => {
    req.reply({ statusCode: 200, headers: { "Content-Type": "application/proto" }, body });
  }).as(alias);
}

function interceptGenerateTokenFailure(alias = "generateTokenFail") {
  cy.intercept("POST", "**/rpc/token.TokenService/GenerateToken", { statusCode: 500 }).as(alias);
}

// ---------------------------------------------------------------------------
// Specs
// ---------------------------------------------------------------------------

describe("useLiveKitTerminalToken", () => {
  it("fetches an initial token via generateToken and exposes it with its TTL", () => {
    // Given
    interceptGenerateToken(GENERATE_OK);

    // When
    cy.mount(<TokenHookHarness room={ROOM_NAME} identity={IDENTITY} />);
    cy.wait("@generateToken");

    // Then
    byTestId(TOKEN_EL).should("have.text", "lk-initial-token");
    byTestId(TTL_EL).should("have.text", "600");
  });

  it("requests the token scoped to the given room and identity", () => {
    // Given
    interceptGenerateToken(GENERATE_OK);

    // When
    cy.mount(<TokenHookHarness room={ROOM_NAME} identity={IDENTITY} />);

    // Then
    cy.wait("@generateToken").then((interception) => {
      const req = fromBinary(GenerateTokenRequestSchema, decodeProtoRequestBody(interception.request.body));
      expect(req.room).to.equal(ROOM_NAME);
      expect(req.identity).to.equal(IDENTITY);
    });
  });

  it("getToken() calls refreshToken and resolves with the refreshed token", () => {
    // Given
    interceptGenerateToken(GENERATE_OK);
    interceptRefreshToken(REFRESH_OK);
    cy.mount(<TokenHookHarness room={ROOM_NAME} identity={IDENTITY} />);
    cy.wait("@generateToken");

    // When
    byTestId(REFRESH_BTN).click();
    cy.wait("@refreshToken");

    // Then
    byTestId(REFRESHED_TOKEN_EL).should("have.text", "lk-refreshed-token");
  });

  it("surfaces an error when generateToken fails, without throwing", () => {
    // Given
    interceptGenerateTokenFailure();

    // When
    cy.mount(<TokenHookHarness room={ROOM_NAME} identity={IDENTITY} />);

    // Then
    byTestId(ERROR_EL).invoke("text").should("not.equal", "");
    byTestId(TOKEN_EL).should("have.text", "");
  });

  it("sends the signed-in operator's session token with the initial generateToken request", () => {
    // Given a signed-in operator — the daemon's mint refuses an anonymous caller
    interceptGenerateToken(GENERATE_OK);

    // When
    cy.mount(
      withSessionTokenGate(SESSION_TOKEN, <GatedTokenHookHarness room={ROOM_NAME} identity={IDENTITY} />),
    );

    // Then
    cy.wait("@generateToken").then((interception) => {
      const req = fromBinary(GenerateTokenRequestSchema, decodeProtoRequestBody(interception.request.body));
      expect(req.sessionToken).to.equal(SESSION_TOKEN);
    });
  });

  it("sends the signed-in operator's session token with every refreshToken request", () => {
    // Given a signed-in operator whose terminal is already holding a token
    interceptGenerateToken(GENERATE_OK);
    interceptRefreshToken(REFRESH_OK);
    cy.mount(
      withSessionTokenGate(SESSION_TOKEN, <GatedTokenHookHarness room={ROOM_NAME} identity={IDENTITY} />),
    );
    cy.wait("@generateToken");

    // When the terminal renews it
    byTestId(REFRESH_BTN).click();

    // Then the renewal is authenticated too — a lapsed caller cannot keep renewing room admission
    cy.wait("@refreshToken").then((interception) => {
      const req = fromBinary(RefreshTokenRequestSchema, decodeProtoRequestBody(interception.request.body));
      expect(req.sessionToken).to.equal(SESSION_TOKEN);
    });
  });

  it("does not call generateToken when room or identity is not yet known", () => {
    // Given
    interceptGenerateToken(GENERATE_OK);

    // When — identity is missing (session not yet attached)
    cy.mount(<TokenHookHarness room={ROOM_NAME} identity={undefined} />);

    // Then — no request is ever made
    cy.get("@generateToken.all").should("have.length", 0);
    byTestId(TOKEN_EL).should("have.text", "");
  });
});
