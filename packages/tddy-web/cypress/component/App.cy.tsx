import React from "react";
import { create } from "@bufbuild/protobuf";
import { toBinary } from "@bufbuild/protobuf";
import { App } from "../../src/index";
import {
  GenerateTokenResponseSchema,
  RefreshTokenResponseSchema,
} from "../../src/gen/token_pb";
import {
  GetAuthStatusResponseSchema,
  GitHubUserSchema,
  GetAuthUrlResponseSchema,
  ExchangeCodeResponseSchema,
} from "../../src/gen/auth_pb";

function mockTokenResponse(token: string, ttlSeconds: number) {
  const msg = create(GenerateTokenResponseSchema, {
    token,
    ttlSeconds: BigInt(ttlSeconds),
  });
  return toBinary(GenerateTokenResponseSchema, msg);
}

function mockRefreshResponse(token: string, ttlSeconds: number) {
  const msg = create(RefreshTokenResponseSchema, {
    token,
    ttlSeconds: BigInt(ttlSeconds),
  });
  return toBinary(RefreshTokenResponseSchema, msg);
}

function mockAuthStatusAuthenticated() {
  const user = create(GitHubUserSchema, {
    login: "testuser",
    avatarUrl: "https://example.com/avatar.png",
    name: "Test User",
    id: BigInt(42),
  });
  const msg = create(GetAuthStatusResponseSchema, {
    authenticated: true,
    user,
  });
  return toBinary(GetAuthStatusResponseSchema, msg);
}

function mockAuthStatusUnauthenticated() {
  const msg = create(GetAuthStatusResponseSchema, {
    authenticated: false,
  });
  return toBinary(GetAuthStatusResponseSchema, msg);
}

const toArrayBuffer = (u8: Uint8Array) => {
  const buf = new ArrayBuffer(u8.length);
  new Uint8Array(buf).set(u8);
  return buf;
};

function interceptAuthAsAuthenticated() {
  const body = mockAuthStatusAuthenticated();
  cy.intercept("POST", "**/rpc/auth.AuthService/GetAuthStatus", (req) => {
    req.reply({
      statusCode: 200,
      headers: { "Content-Type": "application/proto" },
      body: toArrayBuffer(body),
    });
  }).as("getAuthStatus");
}

function interceptAuthAsUnauthenticated() {
  const body = mockAuthStatusUnauthenticated();
  cy.intercept("POST", "**/rpc/auth.AuthService/GetAuthStatus", (req) => {
    req.reply({
      statusCode: 200,
      headers: { "Content-Type": "application/proto" },
      body: toArrayBuffer(body),
    });
  }).as("getAuthStatus");
}

describe("App", () => {
  beforeEach(() => {
    cy.clearLocalStorage();
    cy.clearAllSessionStorage();
  });

  it("shows login button when not authenticated", () => {
    interceptAuthAsUnauthenticated();
    cy.mount(<App />);
    cy.get("[data-testid='github-login-button']", { timeout: 5000 }).should("exist");
    cy.get("#livekit-url").should("not.exist");
  });

  it("shows identity, url, and roomName form fields when authenticated", () => {
    window.localStorage.setItem("tddy_session_token", "fake-token");
    interceptAuthAsAuthenticated();
    cy.mount(<App />);
    cy.wait("@getAuthStatus");
    cy.get("#livekit-url", { timeout: 5000 }).should("exist");
    cy.get("[data-testid='livekit-identity']").should("exist");
    cy.get("#livekit-room").should("exist");
    cy.get("[data-testid='user-login']").should("have.text", "testuser");
  });

  it("connects via Connect-RPC token fetch when authenticated and form submitted", () => {
    window.localStorage.setItem("tddy_session_token", "fake-token");
    interceptAuthAsAuthenticated();

    const mockToken = "mock-jwt-from-rpc";
    const mockTtl = 600;
    const generateBody = mockTokenResponse(mockToken, mockTtl);
    const refreshBody = mockRefreshResponse(mockToken, mockTtl);
    cy.intercept("POST", "**/rpc/token.TokenService/GenerateToken", (req) => {
      req.reply({
        statusCode: 200,
        headers: { "Content-Type": "application/proto" },
        body: toArrayBuffer(generateBody),
      });
    }).as("generateToken");
    cy.intercept("POST", "**/rpc/token.TokenService/RefreshToken", (req) => {
      req.reply({
        statusCode: 200,
        headers: { "Content-Type": "application/proto" },
        body: toArrayBuffer(refreshBody),
      });
    }).as("refreshToken");

    cy.mount(<App />);
    cy.wait("@getAuthStatus");
    cy.get("#livekit-url", { timeout: 5000 }).type("ws://localhost:7880");
    cy.get("[data-testid='livekit-identity']").type("client");
    cy.get("#livekit-room").clear().type("terminal-e2e");
    cy.get("button[type='submit']").click();

    cy.wait("@generateToken");
    cy.get("[data-testid='livekit-status']", { timeout: 5000 }).should("exist");
    cy.get("#livekit-url").should("not.exist");
  });
});
