/**
 * RPC Playground — ServerReflection acceptance tests over real LiveKit.
 *
 * These tests verify that the standard gRPC ServerReflection bidi stream works
 * over the existing LiveKitTransport, and that the resulting runtime descriptor
 * allows invoking EchoService methods dynamically (no compiled ConnectES client).
 *
 * A LiveKit server is started automatically when LIVEKIT_TESTKIT_WS_URL is not set
 * (Docker required). Set LIVEKIT_TESTKIT_WS_URL=ws://... to reuse an existing instance.
 */

import React from "react";
import { ReflectionTestHarness } from "./support/ReflectionTestHarness";

/** Deduplicate logs while preserving order (React Strict Mode safety). */
function dedupeLogs(logs: string[]): string[] {
  const seen = new Set<string>();
  return logs.filter((l) => {
    if (seen.has(l)) return false;
    seen.add(l);
    return true;
  });
}

describe("ServerReflection over LiveKit", () => {
  let serverUrl: string;
  let clientToken: string;

  before(() => {
    // startEchoServerWithReflection: starts an echo server that ALSO registers
    // grpc.reflection.v1.ServerReflection as a ServiceEntry.
    // This task does not exist until the green phase.
    return cy
      .task("startEchoServerWithReflection")
      .then((result: { url: string; clientToken: string }) => {
        serverUrl = result.url;
        clientToken = result.clientToken;
      });
  });

  after(() => {
    cy.task("stopEchoServer");
  });

  it("discovers EchoService via ServerReflectionInfo bidi stream", () => {
    cy.mount(
      <ReflectionTestHarness
        url={serverUrl}
        token={clientToken}
        action="list-services"
      />
    );
    cy.get('[data-test="status"]').should("contain", "done");
    cy.window().then((win) => {
      const logs = dedupeLogs((win as unknown as { cypressLogs?: string[] }).cypressLogs ?? []);
      const listResult = logs.find((l) => l.includes("[REFLECTION] list_services:"));
      expect(listResult, "list_services response log").to.exist;
      expect(listResult).to.include("test.EchoService");
      // grpc.reflection.v1.ServerReflection itself must also appear
      expect(listResult).to.include("grpc.reflection.v1.ServerReflection");
    });
  });

  it("fetches Echo method descriptor and invokes it dynamically without a compiled client", () => {
    cy.mount(
      <ReflectionTestHarness
        url={serverUrl}
        token={clientToken}
        action="dynamic-invoke-echo"
        message="playground-test"
      />
    );
    cy.get('[data-test="status"]').should("contain", "done");
    cy.window().then((win) => {
      const logs = dedupeLogs((win as unknown as { cypressLogs?: string[] }).cypressLogs ?? []);
      const resultLog = logs.find((l) => l.includes("[REFLECTION] invoke result:"));
      expect(resultLog, "invoke result log").to.exist;
      expect(resultLog).to.include("playground-test");
    });
  });

  it("invokes EchoServerStream dynamically and receives 3 decoded chunks", () => {
    cy.mount(
      <ReflectionTestHarness
        url={serverUrl}
        token={clientToken}
        action="dynamic-invoke-server-stream"
        message="streaming"
      />
    );
    cy.get('[data-test="status"]').should("contain", "done");
    cy.window().then((win) => {
      const logs = dedupeLogs((win as unknown as { cypressLogs?: string[] }).cypressLogs ?? []);
      const chunkLogs = logs.filter((l) => l.includes("[REFLECTION] stream chunk:"));
      expect(chunkLogs).to.have.length(3);
      expect(chunkLogs[0]).to.include("streaming #1");
      expect(chunkLogs[1]).to.include("streaming #2");
      expect(chunkLogs[2]).to.include("streaming #3");
    });
  });

  it("invokes EchoBidiStream dynamically and receives echoed messages", () => {
    cy.mount(
      <ReflectionTestHarness
        url={serverUrl}
        token={clientToken}
        action="dynamic-invoke-bidi-stream"
        message="a b c"
      />
    );
    cy.get('[data-test="status"]').should("contain", "done");
    cy.window().then((win) => {
      const logs = dedupeLogs((win as unknown as { cypressLogs?: string[] }).cypressLogs ?? []);
      const chunkLogs = logs.filter((l) => l.includes("[REFLECTION] stream chunk:"));
      expect(chunkLogs).to.have.length(3);
      expect(chunkLogs[0]).to.include("a");
      expect(chunkLogs[1]).to.include("b");
      expect(chunkLogs[2]).to.include("c");
    });
  });
});
