import React, { useMemo } from "react";
import { createClient } from "@connectrpc/connect";
import { createConnectTransport } from "@connectrpc/connect-web";
import { fromBinary } from "@bufbuild/protobuf";
import { GhosttyTerminalLiveKit } from "../../src/components/GhosttyTerminalLiveKit";
import {
  ConnectionService,
  Signal,
  SignalSessionRequestSchema,
} from "../../src/gen/connection_pb";

function createConnectionClient() {
  const transport = createConnectTransport({
    baseUrl: `${window.location.origin}/rpc`,
    useBinaryFormat: true,
  });
  return createClient(ConnectionService, transport);
}

const TERMINATE_TEST_SESSION_ID = "ghostty-ct-terminate-session";

describe("GhosttyTerminalLiveKit", () => {
  const getToken = () => Promise.resolve({ token: "fake-token", ttlSeconds: BigInt(600) });

  it("shows mobile keyboard overlay when showMobileKeyboard is true regardless of preventFocusOnTap", () => {
    cy.mount(
      <div style={{ height: 400, position: "relative" }}>
        <GhosttyTerminalLiveKit
          url="ws://localhost:9999"
          token="fake-token"
          getToken={getToken}
          ttlSeconds={BigInt(600)}
          showMobileKeyboard
          preventFocusOnTap={false}
        />
      </div>
    );
    cy.get("[data-testid='mobile-keyboard-button']", { timeout: 10000 }).should("exist");
    cy.get("[data-testid='mobile-keyboard-button']").should("contain.text", "Keyboard");
  });

  it("mobile keyboard overlay input forwards typed characters when focused", () => {
    cy.mount(
      <div style={{ height: 400, position: "relative" }}>
        <GhosttyTerminalLiveKit
          url="ws://localhost:9999"
          token="fake-token"
          getToken={getToken}
          ttlSeconds={BigInt(600)}
          showMobileKeyboard
          preventFocusOnTap={false}
        />
      </div>
    );
    cy.get("[data-testid='mobile-keyboard-button']", { timeout: 10000 }).should("exist");
    cy.get("[data-testid='mobile-keyboard-button']").within(() => {
      cy.get("input").focus();
    });
    cy.get("[data-testid='mobile-keyboard-button']").within(() => {
      cy.get("input").type("x");
    });
    cy.get("[data-testid='mobile-keyboard-button']").within(() => {
      cy.get("input").should("have.value", "");
    });
  });

  it("does not show mobile keyboard overlay when showMobileKeyboard is false", () => {
    cy.mount(
      <div style={{ height: 400, position: "relative" }}>
        <GhosttyTerminalLiveKit
          url="ws://localhost:9999"
          token="fake-token"
          getToken={getToken}
          ttlSeconds={BigInt(600)}
          showMobileKeyboard={false}
        />
      </div>
    );
    cy.get("[data-testid='mobile-keyboard-button']").should("not.exist");
  });

  describe("fullscreen overlay — Terminate (SIGINT)", () => {
    it("GhosttyTerminalLiveKit shows Terminate when session terminate handler is provided", () => {
      cy.mount(
        <div style={{ height: 400, position: "relative" }}>
          <GhosttyTerminalLiveKit
            url="ws://localhost:9999"
            token="fake-token"
            getToken={getToken}
            ttlSeconds={BigInt(600)}
            connectionOverlay={{
              onDisconnect: () => {},
              buildId: "test-build",
              onSessionTerminate: () => {},
            }}
          />
        </div>
      );
      cy.get("[data-testid='terminate-button']", { timeout: 10000 }).should("exist");
      cy.get("[data-testid='terminate-button']")
        .should("have.attr", "aria-label")
        .and("match", /daemon/i);
      cy.get("[data-testid='terminate-button']").should("have.attr", "data-sigint-wired", "true");
      cy.get("[data-testid='ctrl-c-button']").should("exist");
      cy.get("[data-testid='disconnect-button']").should("exist");
    });

    it("GhosttyTerminalLiveKit hides Terminate when session terminate handler is omitted", () => {
      cy.mount(
        <div style={{ height: 400, position: "relative" }}>
          <GhosttyTerminalLiveKit
            url="ws://localhost:9999"
            token="fake-token"
            getToken={getToken}
            ttlSeconds={BigInt(600)}
            connectionOverlay={{
              onDisconnect: () => {},
              buildId: "test-build",
            }}
          />
        </div>
      );
      cy.get("[data-testid='terminate-button']").should("not.exist");
      cy.get("[data-testid='ctrl-c-button']").should("exist");
      cy.get("[data-testid='disconnect-button']").should("exist");
    });

    it("GhosttyTerminalLiveKit Terminate click sends SignalSession with SIGINT", () => {
      window.localStorage.setItem("tddy_session_token", "ct-fake-session-token");
      cy.intercept("POST", "**/rpc/connection.ConnectionService/SignalSession", (req) => {
        const raw = req.body;
        const ab =
          raw instanceof ArrayBuffer
            ? raw
            : typeof raw === "string"
              ? new TextEncoder().encode(raw).buffer
              : (raw as ArrayBuffer);
        const decoded = fromBinary(SignalSessionRequestSchema, new Uint8Array(ab));
        expect(decoded.sessionId).to.eq(TERMINATE_TEST_SESSION_ID);
        expect(decoded.signal).to.eq(Signal.SIGINT);
        expect(decoded.sessionToken).to.eq("ct-fake-session-token");
        req.reply({
          statusCode: 200,
          headers: { "Content-Type": "application/proto" },
          body: new Uint8Array([0x08, 0x01]).buffer,
        });
      }).as("signalSessionTerminate");

      function TerminateRpcWrapper() {
        const client = useMemo(() => createConnectionClient(), []);
        const onSessionTerminate = () =>
          client.signalSession({
            sessionToken: window.localStorage.getItem("tddy_session_token") ?? "",
            sessionId: TERMINATE_TEST_SESSION_ID,
            signal: Signal.SIGINT,
          });
        return (
          <div style={{ height: 400, position: "relative" }}>
            <GhosttyTerminalLiveKit
              url="ws://localhost:9999"
              token="fake-token"
              getToken={getToken}
              ttlSeconds={BigInt(600)}
              connectionOverlay={{
                onDisconnect: () => {},
                buildId: "test-build",
                onSessionTerminate,
              }}
            />
          </div>
        );
      }

      cy.mount(<TerminateRpcWrapper />);
      cy.get("[data-testid='terminate-button']", { timeout: 10000 }).click();
      cy.wait("@signalSessionTerminate");
      cy.get("[data-testid='terminate-rpc-complete']").should("exist");
    });
  });
});
