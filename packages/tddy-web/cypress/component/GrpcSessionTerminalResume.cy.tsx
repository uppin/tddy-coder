/**
 * Behaviour spec: a Sessions-screen gRPC terminal (GrpcSessionTerminal) survives a transient
 * transport blip and resumes with `FROM_OFFSET` instead of re-replaying.
 *
 * Expected:
 *   1. On first connect the client opens `StreamTerminalOutput` with `mode = TAIL`; the replay
 *      frame's `endOffset` is tracked as the client's `currentOffset`.
 *   2. When the daemon client goes `null` (transport blip) the stream ends but `onDisconnect` is
 *      NOT invoked — the terminal stays mounted (its scrollback and ghostty instance survive).
 *   3. When a non-null client returns, the stream re-opens with `mode = FROM_OFFSET` and
 *      `fromOffset = currentOffset`, so the daemon sends only the missed gap — no duplicate replay.
 *
 * The backend is an in-memory fake client (no `cy.intercept`): the test drives the async iterable
 * returned by `streamTerminalOutput` directly, so it can push frames and assert the request
 * payload on each open without HTTP plumbing.
 */

import React, { useMemo, useState } from "react";
import { create, type MessageShape } from "@bufbuild/protobuf";
import { type Client } from "@connectrpc/connect";
import {
  ConnectionService,
  SendTerminalInputResponseSchema,
  SessionTerminalOutputSchema,
  type SessionTerminalOutput,
  type StreamTerminalOutputRequest,
  StreamReplayMode,
} from "../../src/gen/connection_pb";
import { GrpcSessionTerminal } from "../../src/components/sessions/GrpcSessionTerminal";
import { UploadProgressProvider } from "../../src/rpc/uploadProgress";
import { byTestId, TEST_IDS } from "../support/testIds";

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

const SESSION_ID = "resume-test-session-4f3a";
const SESSION_TOKEN = "test-session-token-resume";
const FIRST_TIP_OFFSET = 10n;
const BLIP_BTN = "resume-test-blip";
const RESTORE_BTN = "resume-test-restore";

type ConnectionClient = Client<typeof ConnectionService>;

// ---------------------------------------------------------------------------
// In-memory backend doubles
// ---------------------------------------------------------------------------

/** A captured `StreamTerminalOutput` open: the request and a controller to push frames / end. */
interface CapturedStream {
  request: StreamTerminalOutputRequest;
  pushFrame(frame: SessionTerminalOutput): void;
  end(): void;
}

/** A fake ConnectionService client that records each `streamTerminalOutput` open and lets the
 *  test push frames into the async iterable it returns. `sendTerminalInput` resolves OK. */
function makeFakeClient() {
  const opens: CapturedStream[] = [];

  function streamTerminalOutput(req: StreamTerminalOutputRequest): AsyncIterable<SessionTerminalOutput> {
    const frames: SessionTerminalOutput[] = [];
    let ended = false;
    let pending: ((value: IteratorResult<SessionTerminalOutput>) => void) | null = null;

    const iterator: AsyncIterator<SessionTerminalOutput> = {
      next(): Promise<IteratorResult<SessionTerminalOutput>> {
        if (frames.length > 0) {
          return Promise.resolve({ value: frames.shift() as SessionTerminalOutput, done: false });
        }
        if (ended) {
          return Promise.resolve({ value: undefined as unknown as SessionTerminalOutput, done: true });
        }
        return new Promise<IteratorResult<SessionTerminalOutput>>((resolve) => {
          pending = (value) => {
            pending = null;
            resolve(value);
          };
        });
      },
      return(): Promise<IteratorResult<SessionTerminalOutput>> {
        ended = true;
        pending?.({ value: undefined as unknown as SessionTerminalOutput, done: true });
        return Promise.resolve({ value: undefined as unknown as SessionTerminalOutput, done: true });
      },
      throw(e?: any): Promise<IteratorResult<SessionTerminalOutput>> {
        return Promise.reject(e);
      },
    };

    const controller: CapturedStream = {
      request: req,
      pushFrame(frame: SessionTerminalOutput) {
        frames.push(frame);
        if (pending) {
          const resolve = pending;
          pending = null;
          resolve({ value: frames.shift() as SessionTerminalOutput, done: false });
        }
      },
      end() {
        ended = true;
        pending?.({ value: undefined as unknown as SessionTerminalOutput, done: true });
      },
    };
    opens.push(controller);

    const iterable: AsyncIterable<SessionTerminalOutput> = {
      [Symbol.asyncIterator]() {
        return iterator;
      },
    };
    return iterable;
  }

  function sendTerminalInput(): Promise<MessageShape<typeof SendTerminalInputResponseSchema>> {
    return Promise.resolve(create(SendTerminalInputResponseSchema, {}));
  }

  // The client object only needs the methods GrpcSessionTerminal calls; cast to the typed Client.
  const client = {
    streamTerminalOutput,
    sendTerminalInput,
    // Unused by this test but present so the cast is shape-compatible.
    getTerminalHistory: () => asyncIterableEmpty(),
  } as unknown as ConnectionClient;

  return { client, opens };
}

function asyncIterableEmpty<T>(): AsyncIterable<T> {
  return { [Symbol.asyncIterator]() { return { next: () => Promise.resolve({ value: undefined as unknown as T, done: true }) }; } };
}

// ---------------------------------------------------------------------------
// Fluent driver
// ---------------------------------------------------------------------------

function aResumeTerminal() {
  const fake = makeFakeClient();
  const onDisconnect = cy.stub().as("onDisconnect");

  function ResumeHarness() {
    const [client, setClient] = useState<ConnectionClient | null>(fake.client);
    // Keep the same fake client reference across blip/restore so the second open is recorded
    // against the same in-memory backend.
    const stableFake = useMemo(() => fake, [fake]);
    return (
      <div style={{ width: 800, height: 400, position: "relative" }}>
        <button type="button" data-testid={BLIP_BTN} onClick={() => setClient(null)}>
          blip
        </button>
        <button type="button" data-testid={RESTORE_BTN} onClick={() => setClient(stableFake.client)}>
          restore
        </button>
        <UploadProgressProvider>
          <GrpcSessionTerminal
            sessionId={SESSION_ID}
            sessionToken={SESSION_TOKEN}
            client={client}
            connected={null}
            onDisconnect={onDisconnect}
          />
        </UploadProgressProvider>
      </div>
    );
  }

  const driver = {
    mount() {
      cy.mount(<ResumeHarness />);
      return driver;
    },
    expectTerminalVisible() {
      byTestId(TEST_IDS.ghosttyTerminal, { timeout: 10000 }).should("exist");
      return driver;
    },
    /** First (TAIL) open arrives; push a replay frame tagged with the tip offset so the client
     *  records `currentOffset = FIRST_TIP_OFFSET` and `hasSynced = true`. */
    deliverInitialTailFrame() {
      cy.then(() => {
        expect(fake.opens).to.have.lengthOf(1, "one TAIL open on mount");
        expect(fake.opens[0].request.mode).to.equal(StreamReplayMode.TAIL, "first open is TAIL");
        fake.opens[0].pushFrame(
          create(SessionTerminalOutputSchema, {
            data: new TextEncoder().encode("initial"),
            endOffset: FIRST_TIP_OFFSET,
            atOldest: false,
            // Stamped with the session and resolved terminal, as the daemon stamps every frame.
            sessionId: SESSION_ID,
            terminalId: "main",
          }),
        );
      });
      return driver;
    },
    simulateTransportBlip() {
      byTestId(BLIP_BTN).click();
      return driver;
    },
    restoreClient() {
      byTestId(RESTORE_BTN).click();
      return driver;
    },
    /** The reconnect open must carry `FROM_OFFSET` with the tracked `currentOffset` — no duplicate. */
    expectReconnectResumesFromOffset() {
      cy.then(() => {
        expect(fake.opens.length, "a second open after restore").to.be.greaterThan(1);
        const reconnect = fake.opens[1].request;
        expect(reconnect.mode).to.equal(StreamReplayMode.FROM_OFFSET, "reconnect is FROM_OFFSET");
        expect(reconnect.fromOffset).to.equal(FIRST_TIP_OFFSET, "fromOffset == tracked currentOffset");
      });
      return driver;
    },
    expectTerminalSurvivedBlip() {
      byTestId(TEST_IDS.ghosttyTerminal).should("exist");
      cy.get("@onDisconnect").should("not.have.been.called");
      return driver;
    },
  };
  return driver;
}

// ---------------------------------------------------------------------------
// Specs
// ---------------------------------------------------------------------------

describe("GrpcSessionTerminal — transport blip resumes with FROM_OFFSET, terminal survives", () => {
  it("opens TAIL on first connect, then FROM_OFFSET with the tracked offset after a blip", () => {
    // Given — a freshly mounted gRPC terminal
    aResumeTerminal()
      .mount()
      .expectTerminalVisible()
      // When — the first TAIL replay frame lands (carrying the tip offset)
      .deliverInitialTailFrame()
      // And — the daemon client goes null (transient transport blip)
      .simulateTransportBlip()
      // Then — the terminal stays mounted and disconnect does NOT fire (blip, not pty_done)
      .expectTerminalSurvivedBlip()
      // When — a non-null client returns
      .restoreClient()
      // Then — the stream re-opens with FROM_OFFSET at the tracked offset (no duplicate replay)
      .expectReconnectResumesFromOffset()
      .expectTerminalSurvivedBlip();
  });
});
