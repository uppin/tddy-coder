/**
 * Acceptance tests for the envelope transport core: the parts every flavour shares, driven over an
 * in-memory frame pipe.
 *
 * See `docs/dev/1-WIP/2026-09-04-tauri-desktop-single-process.md` for the design this validates.
 */

import { describe, it, expect } from "bun:test";
import { Code, createClient, type ConnectError } from "@connectrpc/connect";
import { createEnvelopeTransport } from "./envelope-transport.js";
import { EchoService } from "./gen/test/echo_service_pb.js";
import { aFramePipe, anEchoResponseBody } from "./test-utils/framePipeDouble.js";

const ECHO_SERVICE = "test.EchoService";

describe("envelope transport", () => {
  it("encodes one request frame per unary call with an incrementing request id", async () => {
    // Given a transport on a fresh connection
    const pipe = aFramePipe();
    const client = createClient(EchoService, createEnvelopeTransport({ pipe }));

    // When two unary calls are issued before either is answered
    const first = client.echo({ message: "one" });
    const second = client.echo({ message: "two" });

    // Then each got its own frame, naming the call, with ids from this connection's own space
    const sent = pipe.sentRequests();
    expect(sent.map((request) => request.requestId)).toEqual([1, 2]);
    expect(sent.map((request) => request.callMetadata?.method)).toEqual(["Echo", "Echo"]);
    expect(sent.map((request) => request.callMetadata?.service)).toEqual([
      ECHO_SERVICE,
      ECHO_SERVICE,
    ]);

    // Settle both calls so neither promise outlives the test.
    for (const request of sent) {
      pipe.respond({
        requestId: request.requestId,
        clientEpoch: request.clientEpoch,
        callMetadata: { service: ECHO_SERVICE, method: "Echo" },
        responseMessage: anEchoResponseBody("answered"),
        endOfStream: true,
      });
    }
    await Promise.all([first, second]);
  });

  it("settles a call only with the response frame that names its own method", async () => {
    // Given a unary call in flight
    const pipe = aFramePipe();
    const client = createClient(EchoService, createEnvelopeTransport({ pipe }));
    const pending = client.echo({ message: "mine" });
    const [request] = pipe.sentRequests();

    // When a frame holding this id answers a different method, followed by one that answers ours
    pipe.respond({
      requestId: request.requestId,
      clientEpoch: request.clientEpoch,
      callMetadata: { service: ECHO_SERVICE, method: "EchoServerStream" },
      responseMessage: anEchoResponseBody("not-mine"),
      endOfStream: true,
    });
    pipe.respond({
      requestId: request.requestId,
      clientEpoch: request.clientEpoch,
      callMetadata: { service: ECHO_SERVICE, method: "Echo" },
      responseMessage: anEchoResponseBody("mine"),
      endOfStream: true,
    });

    // Then the call resolves with its own method's response
    expect((await pending).message).toEqual("mine");
  });

  it("mints a non-zero client epoch for the connection", async () => {
    // Given a transport on a fresh connection
    const pipe = aFramePipe();
    const client = createClient(EchoService, createEnvelopeTransport({ pipe }));

    // When it issues a call
    const pending = client.echo({ message: "one" });
    const [request] = pipe.sentRequests();

    // Then the frame carries an epoch — zero on the wire means the field was absent, so it can
    // never be this connection's identity
    expect(request.clientEpoch).not.toEqual(0);

    pipe.respond({
      requestId: request.requestId,
      clientEpoch: request.clientEpoch,
      callMetadata: request.callMetadata,
      responseMessage: anEchoResponseBody("answered"),
      endOfStream: true,
    });
    await pending;
  });

  it("carries the caller-supplied client epoch on every request frame", async () => {
    // Given a transport built for a known connection
    const pipe = aFramePipe();
    const client = createClient(
      EchoService,
      createEnvelopeTransport({ pipe, clientEpoch: 4242 }),
    );

    // When it issues two calls
    const first = client.echo({ message: "one" });
    const second = client.echo({ message: "two" });

    // Then both name that connection
    const sent = pipe.sentRequests();
    expect(sent.map((request) => request.clientEpoch)).toEqual([4242, 4242]);

    for (const request of sent) {
      pipe.respond({
        requestId: request.requestId,
        clientEpoch: 4242,
        callMetadata: request.callMetadata,
        responseMessage: anEchoResponseBody("answered"),
        endOfStream: true,
      });
    }
    await Promise.all([first, second]);
  });

  it("ignores a response frame whose request id no call holds", async () => {
    // Given a unary call in flight
    const pipe = aFramePipe();
    const client = createClient(EchoService, createEnvelopeTransport({ pipe }));
    const pending = client.echo({ message: "mine" });
    const [request] = pipe.sentRequests();

    // When a frame arrives for an id this connection never issued, then this call's own answer
    pipe.respond({
      requestId: request.requestId + 500,
      clientEpoch: request.clientEpoch,
      callMetadata: request.callMetadata,
      responseMessage: anEchoResponseBody("nobody-asked"),
      endOfStream: true,
    });
    pipe.respond({
      requestId: request.requestId,
      clientEpoch: request.clientEpoch,
      callMetadata: request.callMetadata,
      responseMessage: anEchoResponseBody("mine"),
      endOfStream: true,
    });

    // Then the unclaimed frame changed nothing and the call resolved normally
    expect((await pending).message).toEqual("mine");
  });

  it("settles every pending call when the pipe closes", async () => {
    // Given two calls in flight
    const pipe = aFramePipe();
    const client = createClient(EchoService, createEnvelopeTransport({ pipe }));
    const first = client.echo({ message: "one" });
    const second = client.echo({ message: "two" });

    // When the pipe goes away without answering either
    pipe.closeWith("the pipe went away");

    // Then neither call is left hanging. `null` on success, so a resolved call fails the
    // assertions below rather than passing them.
    const failures = await Promise.all([
      first.then(
        () => null,
        (error: ConnectError) => error,
      ),
      second.then(
        () => null,
        (error: ConnectError) => error,
      ),
    ]);
    expect(failures.map((failure) => failure?.code)).toEqual([
      Code.Unavailable,
      Code.Unavailable,
    ]);
  });
});
