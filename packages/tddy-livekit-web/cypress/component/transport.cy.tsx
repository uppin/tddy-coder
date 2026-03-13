import React from "react";
import { TransportTestHarness } from "../support/TransportTestHarness";

/** Deduplicate logs (e.g. from React Strict Mode double-mount) while preserving order. */
function dedupeLogs(logs: string[]): string[] {
  const seen = new Set<string>();
  return logs.filter((l) => {
    if (seen.has(l)) return false;
    seen.add(l);
    return true;
  });
}

describe("LiveKitTransport", () => {
  let serverUrl: string;
  let clientToken: string;

  before(() => {
    return cy
      .task("startEchoServer")
      .then((result: { url: string; clientToken: string }) => {
        serverUrl = result.url;
        clientToken = result.clientToken;
      });
  });

  after(() => {
    cy.task("stopEchoServer");
  });

  it("handles server streaming", () => {
    cy.mount(
      <TransportTestHarness
        url={serverUrl}
        token={clientToken}
        action="server-stream"
        message="streaming"
      />
    );
    cy.get('[data-test="status"]').should("contain", "done");
    cy.window().then((win) => {
      const logs = dedupeLogs((win as unknown as { cypressLogs?: string[] }).cypressLogs ?? []);
      const streamLogs = logs.filter((l) => l.includes("[TEST] stream message"));
      expect(streamLogs).to.have.length(3);
      expect(streamLogs[0]).to.include("streaming #1");
      expect(streamLogs[1]).to.include("streaming #2");
      expect(streamLogs[2]).to.include("streaming #3");
    });
  });

  it("handles unary echo", () => {
    cy.mount(
      <TransportTestHarness
        url={serverUrl}
        token={clientToken}
        action="unary"
        message="hello world"
      />
    );
    cy.get('[data-test="status"]').should("contain", "done");
    cy.window().then((win) => {
      const logs = dedupeLogs((win as unknown as { cypressLogs?: string[] }).cypressLogs ?? []);
      const resultLog = logs.find((l) => l.includes("[TEST] echo result:"));
      expect(resultLog).to.include("hello world");
    });
  });

  it("handles client streaming", () => {
    cy.mount(
      <TransportTestHarness
        url={serverUrl}
        token={clientToken}
        action="client-stream"
        message="hello world"
      />
    );
    cy.get('[data-test="status"]').should("contain", "done");
    cy.window().then((win) => {
      const logs = dedupeLogs((win as unknown as { cypressLogs?: string[] }).cypressLogs ?? []);
      const resultLog = logs.find((l) => l.includes("[TEST] echo result:"));
      expect(resultLog).to.include("hello | world");
    });
  });

  it("handles bidi streaming", () => {
    cy.mount(
      <TransportTestHarness
        url={serverUrl}
        token={clientToken}
        action="bidi-stream"
        message="a b c"
      />
    );
    cy.get('[data-test="status"]').should("contain", "done");
    cy.window().then((win) => {
      const logs = dedupeLogs((win as unknown as { cypressLogs?: string[] }).cypressLogs ?? []);
      const streamLogs = logs.filter((l) => l.includes("[TEST] stream message"));
      expect(streamLogs).to.have.length(3);
      expect(streamLogs[0]).to.include("a");
      expect(streamLogs[1]).to.include("b");
      expect(streamLogs[2]).to.include("c");
    });
  });

  it("handles error (unknown method)", () => {
    cy.mount(
      <TransportTestHarness
        url={serverUrl}
        token={clientToken}
        action="error"
        message=""
      />
    );
    cy.get('[data-test="status"]').should("contain", "error");
    cy.window().then((win) => {
      const logs = dedupeLogs((win as unknown as { cypressLogs?: string[] }).cypressLogs ?? []);
      const errorLog = logs.find((l) => l.includes("[TEST] error:"));
      expect(errorLog).to.exist;
      expect(errorLog).to.match(/NOT_FOUND|Unknown|FakeMethod|unknown/i);
    });
  });

  it("handles AbortSignal cancellation", () => {
    cy.mount(
      <TransportTestHarness
        url={serverUrl}
        token={clientToken}
        action="abort"
        message="abort-me"
      />
    );
    cy.get('[data-test="status"]').should("contain", "error");
    cy.window().then((win) => {
      const logs = dedupeLogs((win as unknown as { cypressLogs?: string[] }).cypressLogs ?? []);
      const transportError = logs.find((l) =>
        l.includes("[LiveKitTransport]") && l.includes("cancelled")
      );
      const testError = logs.find((l) => l.includes("[TEST] error:"));
      expect(transportError).to.exist;
      expect(testError).to.exist;
      expect(testError).to.include("cancelled");
    });
  });
});
