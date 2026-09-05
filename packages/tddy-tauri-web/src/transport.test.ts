/**
 * Acceptance tests for the webview-IPC transport: the browser half of the flavour, driven over an
 * in-memory IPC bridge instead of the host application's.
 *
 * See `docs/dev/1-WIP/2026-09-04-tauri-desktop-single-process.md` for the design this validates.
 */

import { describe, it, expect } from "bun:test";
import { Code, createClient, type ConnectError } from "@connectrpc/connect";
import { EchoService } from "tddy-rpc-web/test-fixtures";
import { createTauriTransport } from "./transport.js";
import {
  anEchoResponseBody,
  aWebviewIpcDouble,
  PAGE_RELEASED_THE_CONNECTION,
} from "./test-utils/webviewIpcDouble.js";

describe("webview IPC transport", () => {
  it("resolves a unary call through the webview IPC bridge", async () => {
    // Given a page connected to the host application
    const bridge = aWebviewIpcDouble();
    const client = createClient(EchoService, createTauriTransport({ bridge }));

    // When it calls a unary method and the host answers
    const pending = client.echo({ message: "ping" });
    bridge.answer(await bridge.nextRequest(), "pong");

    // Then the call resolves with the host's response
    expect((await pending).message).toEqual("pong");
  });

  it("yields every server-stream message in the order the host sent them", async () => {
    // Given a page connected to the host application
    const bridge = aWebviewIpcDouble();
    const client = createClient(EchoService, createTauriTransport({ bridge }));

    // When it opens a server stream that the host answers with three messages
    const responses = client.echoServerStream({ message: "go" });
    bridge.stream(await bridge.nextRequest(), ["first", "second", "third"]);

    // Then the iterable yields them in that order and completes
    const received: string[] = [];
    for await (const response of responses) {
      received.push(response.message);
    }
    expect(received).toEqual(["first", "second", "third"]);
  });

  it("surfaces the Connect error code carried by an error frame", async () => {
    // Given a page connected to the host application
    const bridge = aWebviewIpcDouble();
    const client = createClient(EchoService, createTauriTransport({ bridge }));

    // When a call is answered with an error frame
    const pending = client.echo({ message: "ping" });
    bridge.fail(
      await bridge.nextRequest(),
      "NOT_FOUND",
      "Unknown service: test.EchoService",
    );

    // Then the caller sees that code, not a generic failure. `null` on success, so a resolved call
    // fails the assertions below rather than passing them.
    const failure = await pending.then(
      () => null,
      (error: ConnectError) => error,
    );
    expect(failure?.code).toEqual(Code.NotFound);
    expect(failure?.rawMessage).toContain("Unknown service");
  });

  it("ignores response frames minted for a previous page's client epoch", async () => {
    // Given a call in flight on this page's connection
    const bridge = aWebviewIpcDouble();
    const client = createClient(EchoService, createTauriTransport({ bridge }));
    const pending = client.echo({ message: "mine" });
    const request = await bridge.nextRequest();

    // When a frame holding this id arrives from another connection, then this page's own answer.
    // An epoch is minted per page load, so any other value stands for another connection.
    bridge.respond({
      requestId: request.requestId,
      clientEpoch: request.clientEpoch + 1,
      callMetadata: request.callMetadata,
      responseMessage: anEchoResponseBody("stale"),
      endOfStream: true,
    });
    bridge.answer(request, "mine");

    // Then the call resolves with its own connection's response
    expect((await pending).message).toEqual("mine");
  });

  it("rejects a pending call when the IPC channel closes mid-stream", async () => {
    // Given a server stream that has delivered one message and stayed open
    const bridge = aWebviewIpcDouble();
    const client = createClient(EchoService, createTauriTransport({ bridge }));
    const responses = client.echoServerStream({ message: "go" });
    bridge.streamPartially(await bridge.nextRequest(), ["first"]);
    const iterator = responses[Symbol.asyncIterator]();
    expect((await iterator.next()).value?.message).toEqual("first");

    // When the channel goes away before the stream closes
    bridge.closeChannel("the window closed");

    // Then the stream fails rather than hanging. `null` on a clean end, so a completed stream
    // fails the assertion below rather than passing it.
    const failure = await iterator.next().then(
      () => null,
      (error: ConnectError) => error,
    );
    expect(failure?.code).toEqual(Code.Unavailable);
  });

  it("fails a call still in flight when the page releases the connection", async () => {
    // Given a unary call the host has not answered yet
    const bridge = aWebviewIpcDouble();
    const client = createClient(EchoService, createTauriTransport({ bridge }));
    const pending = client.echo({ message: "ping" });
    await bridge.nextRequest();

    // When the page gives the connection up — a session detached while a screen was still loading
    await bridge.close();

    // Then the caller is told the connection is gone rather than waiting for an answer no peer is
    // left to send. `null` on success, so a resolved call fails the assertions below.
    const failure = await pending.then(
      () => null,
      (error: ConnectError) => error,
    );
    expect(failure?.code).toEqual(Code.Unavailable);
    expect(failure?.rawMessage).toEqual(PAGE_RELEASED_THE_CONNECTION);
  });

  it("registers its response channel before sending the first request frame", async () => {
    // Given a page connected to the host application
    const bridge = aWebviewIpcDouble();
    const client = createClient(EchoService, createTauriTransport({ bridge }));

    // When it issues its first call
    const pending = client.echo({ message: "ping" });
    const request = await bridge.nextRequest();

    // Then the channel was already registered — a frame sent before it would be answered into
    // nothing, and the call would never settle
    expect(bridge.connectedEpoch()).toEqual(request.clientEpoch);

    bridge.answer(request, "pong");
    await pending;
  });

  it("carries the epoch it registered with on every request frame", async () => {
    // Given a page connected as a known connection
    const bridge = aWebviewIpcDouble({ clientEpoch: 4242 });
    const client = createClient(EchoService, createTauriTransport({ bridge }));

    // When it issues two calls
    const first = client.echo({ message: "one" });
    const firstRequest = await bridge.nextRequest();
    bridge.answer(firstRequest, "one");
    await first;
    const second = client.echo({ message: "two" });
    const secondRequest = await bridge.nextRequest();
    bridge.answer(secondRequest, "two");
    await second;

    // Then both frames name that connection
    expect([firstRequest.clientEpoch, secondRequest.clientEpoch]).toEqual([4242, 4242]);
  });
});
