import React from "react";
import { create } from "@bufbuild/protobuf";
import { toBinary } from "@bufbuild/protobuf";
import { App } from "../../src/index";
import {
  GenerateTokenResponseSchema,
  RefreshTokenResponseSchema,
} from "../../src/gen/token_pb";

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

describe("App", () => {
  it("shows identity, url, and roomName form fields for token-service flow", () => {
    cy.mount(<App />);
    cy.get("#livekit-url").should("exist");
    cy.get("[data-testid='livekit-identity']").should("exist");
    cy.get("#livekit-room").should("exist");
  });

  it("connects via Connect-RPC token fetch when identity and room provided", () => {
    const mockToken = "mock-jwt-from-rpc";
    const mockTtl = 600;
    const generateBody = mockTokenResponse(mockToken, mockTtl);
    const refreshBody = mockRefreshResponse(mockToken, mockTtl);
    const toArrayBuffer = (u8: Uint8Array) => {
      const buf = new ArrayBuffer(u8.length);
      new Uint8Array(buf).set(u8);
      return buf;
    };
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
    cy.get("#livekit-url").type("ws://localhost:7880");
    cy.get("[data-testid='livekit-identity']").type("client");
    cy.get("#livekit-room").clear().type("terminal-e2e");
    cy.get("button[type='submit']").click();

    cy.wait("@generateToken");
    cy.get("[data-testid='livekit-status']", { timeout: 5000 }).should("exist");
    cy.get("#livekit-url").should("not.exist");
  });
});
