/**
 * The testkit's server-stream fallback, registered globally in `cypress/support/component.ts`:
 * a backend that serves the unary `startSession` but not `streamStartSession` answers the stream by
 * running the unary handler and emitting its answer as the stream's only `result` event.
 *
 * Drives the testkit directly (backend + typed client), without mounting a component.
 */
import { createClient, Code, ConnectError } from "@connectrpc/connect";
import { anInMemoryRpcBackend, type InMemoryRpcBackend } from "tddy-connectrpc-testkit";
import { SessionService } from "../../src/gen/session_pb";

const STUB_SESSION_ID = "stub-session-1";
const REQUESTED_PROJECT_ID = "proj-1";

/** What a caller of `streamStartSession` observed: the events it received, then how the stream ended. */
interface StreamOutcome {
  readonly events: ReadonlyArray<{ readonly case: string | undefined; readonly sessionId?: string }>;
  readonly failure: ConnectError | undefined;
}

function aBackendServingOnlyUnaryStartSession(): InMemoryRpcBackend {
  return anInMemoryRpcBackend().onUnary(SessionService.method.startSession, () => ({
    sessionId: STUB_SESSION_ID,
  }));
}

/** Runs `streamStartSession` for the requested project to its end and reports what it delivered. */
function streamStartSessionOn(backend: InMemoryRpcBackend): Cypress.Chainable<StreamOutcome> {
  const client = createClient(SessionService, backend.transport());
  return cy.wrap(collectOutcome(client.streamStartSession({ projectId: REQUESTED_PROJECT_ID })));
}

async function collectOutcome(
  stream: AsyncIterable<{ event: { case: string | undefined; value?: unknown } }>,
): Promise<StreamOutcome> {
  const events: Array<{ case: string | undefined; sessionId?: string }> = [];
  let failure: ConnectError | undefined;
  try {
    for await (const { event } of stream) {
      const answer = event.case === "result" ? (event.value as { sessionId: string }) : undefined;
      events.push({ case: event.case, sessionId: answer?.sessionId });
    }
  } catch (error) {
    failure = ConnectError.from(error);
  }
  return { events, failure };
}

describe("registerServerStreamFallback: StartSession over a stream", () => {
  it("answers streamStartSession with a single result carrying the unary startSession answer", () => {
    const backend = aBackendServingOnlyUnaryStartSession();

    streamStartSessionOn(backend).should("deep.equal", {
      events: [{ case: "result", sessionId: STUB_SESSION_ID }],
      failure: undefined,
    });
  });

  it("falls back the same way when startSession is implemented through implement(SessionService)", () => {
    const backend = anInMemoryRpcBackend().implement(SessionService, {
      startSession: () => ({ sessionId: STUB_SESSION_ID }),
    });

    streamStartSessionOn(backend).should("deep.equal", {
      events: [{ case: "result", sessionId: STUB_SESSION_ID }],
      failure: undefined,
    });
  });

  it("delivers an explicit streamStartSession implementation's events instead of the fallback", () => {
    const backend = aBackendServingOnlyUnaryStartSession().implement(SessionService, {
      streamStartSession: async function* () {
        yield { event: { case: "result" as const, value: { sessionId: "streamed-session-1" } } };
      },
    });

    streamStartSessionOn(backend).should("deep.equal", {
      events: [{ case: "result", sessionId: "streamed-session-1" }],
      failure: undefined,
    });
  });

  it("never calls the unary startSession handler when the stream is implemented explicitly", () => {
    const backend = aBackendServingOnlyUnaryStartSession().implement(SessionService, {
      streamStartSession: async function* () {
        yield { event: { case: "result" as const, value: { sessionId: "streamed-session-1" } } };
      },
    });

    streamStartSessionOn(backend).then(() => {
      expect(backend.callsTo(SessionService.method.startSession)).to.have.length(0);
    });
  });

  it("fails the stream with the same code when the unary handler throws a ConnectError", () => {
    const backend = anInMemoryRpcBackend().onUnary(SessionService.method.startSession, () => {
      throw new ConnectError("session already exists", Code.AlreadyExists);
    });

    streamStartSessionOn(backend).then(({ events, failure }) => {
      expect(events).to.have.length(0);
      expect(failure?.code).to.equal(Code.AlreadyExists);
    });
  });

  it("records the fallback's request under the unary startSession method", () => {
    const backend = aBackendServingOnlyUnaryStartSession();

    streamStartSessionOn(backend).then(() => {
      const requests = backend.callsTo(SessionService.method.startSession);
      expect(requests.map((request) => request.projectId)).to.deep.equal([REQUESTED_PROJECT_ID]);
    });
  });

  it("gives no fallback to a backend serving neither startSession nor streamStartSession", () => {
    const backend = anInMemoryRpcBackend();

    streamStartSessionOn(backend).then(({ events, failure }) => {
      expect(events).to.have.length(0);
      expect(failure?.code).to.equal(Code.Unimplemented);
    });
  });
});
