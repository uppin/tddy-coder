/**
 * Behaviour spec: `useConnectionStatus` — how React learns that a session connection's status
 * changed.
 *
 * A `SessionConnection` publishes `status` as a value read at the moment it is asked: a room
 * connects, a bridge goes away, and nothing tells React. So this hook samples, and everything the
 * operator sees about a session's handshake — the overlay that covers the whole pane — is what this
 * sampling reports.
 *
 * Two of its properties are load-bearing and neither is visible from the surface it drives. It must
 * hand back the **same object** while the status holds, because otherwise every attached session's
 * runtime re-renders five times a second for ever; and it must stop sampling on unmount, because a
 * screen that navigated away would otherwise keep a timer per session it once showed.
 *
 * Technical: `packages/tddy-web/docs/session-connections.md`.
 */

import React from "react";
import type { Client, Transport } from "@connectrpc/connect";
import type { DescService } from "@bufbuild/protobuf";
import type { SessionConnection } from "../../src/rpc/connections/session";
import type { ConnectionCapability, ConnectionStatus } from "../../src/rpc/connections/types";
import {
  useConnectionStatus,
  type ObservedConnectionStatus,
} from "../../src/rpc/connections/useConnectionStatus";
import { byTestId } from "../support/testIds";

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

const A_SESSION = "session-0001";

/** How long to wait before claiming a stopped sampler really has stopped. */
const SEVERAL_SAMPLE_INTERVALS = 900;

const STATUS_EL = "observed-status";
const ERROR_EL = "observed-error";
const DISTINCT_EL = "distinct-observations";
const READS_EL = "status-reads";

/**
 * A connection whose status the test moves by hand, and which counts how often it is asked.
 *
 * The count is the only way to see that sampling stopped: a hook that keeps its interval running
 * after unmount goes on reading a connection nobody is showing, and nothing else in the DOM says so.
 */
function aConnectionThatIs(status: ConnectionStatus) {
  let current = status;
  let failure: string | null = null;
  let reads = 0;
  const connection: SessionConnection = {
    hostId: "local",
    sessionId: A_SESSION,
    get status(): ConnectionStatus {
      reads += 1;
      return current;
    },
    get error(): string | null {
      return failure;
    },
    capabilities: new Set<ConnectionCapability>(["rpc"]),
    clientFor: <S extends DescService>(): Client<S> => {
      throw new Error("this connection is a status stand-in and issues no calls");
    },
    transport: (): Transport => {
      throw new Error("this connection is a status stand-in and issues no calls");
    },
    close: () => {},
  };
  return {
    connection,
    reads: () => reads,
    becomes(next: ConnectionStatus, why: string | null = null) {
      current = next;
      failure = why;
    },
  };
}

// ---------------------------------------------------------------------------
// Probe
// ---------------------------------------------------------------------------

/**
 * Renders what the hook observed, plus how many *distinct* objects it has handed back.
 *
 * The distinct count is the re-render guard made visible: an unchanged status that produced a new
 * object every sample would climb without the status ever changing.
 */
function StatusProbe({ connection }: { connection: SessionConnection | null }) {
  const observed = useConnectionStatus(connection);
  const seen = React.useRef<ObservedConnectionStatus[]>([]);
  if (!seen.current.includes(observed)) seen.current = [...seen.current, observed];

  return (
    <div>
      <div data-testid={STATUS_EL}>{observed.status}</div>
      <div data-testid={ERROR_EL}>{observed.error ?? "none"}</div>
      <div data-testid={DISTINCT_EL}>{seen.current.length}</div>
    </div>
  );
}

/** Mounts a probe that can be unmounted from the test, and shows the connection's read count. */
function DismissableStatusProbe({ connection }: { connection: SessionConnection }) {
  const [shown, setShown] = React.useState(true);
  return (
    <div>
      {shown && <StatusProbe connection={connection} />}
      <button type="button" data-testid="dismiss" onClick={() => setShown(false)}>
        dismiss
      </button>
    </div>
  );
}

// ---------------------------------------------------------------------------
// Specs
// ---------------------------------------------------------------------------

describe("watching a session connection's status", () => {
  it("reports what the connection already says, without waiting for a sample", () => {
    // Given a connection that is up before anything mounts — the drawer's fast-path select, which
    // restores an attachment whose room has been joined for minutes
    const live = aConnectionThatIs("connected");

    // When a component starts watching it
    cy.mount(<StatusProbe connection={live.connection} />);

    // Then it reads connected on the very first render. A first sample scheduled for the interval
    // would flash the handshake overlay over a session that never handshook
    byTestId(STATUS_EL).should("have.text", "connected");
  });

  it("picks up a transition the connection never announced", () => {
    // Given a connection still coming up
    const live = aConnectionThatIs("connecting");
    cy.mount(<StatusProbe connection={live.connection} />);
    byTestId(STATUS_EL).should("have.text", "connecting");

    // When its room joins — nothing calls back, nothing re-renders
    cy.then(() => live.becomes("connected"));

    // Then the watcher notices anyway, which is the whole reason it samples
    byTestId(STATUS_EL).should("have.text", "connected");
  });

  it("carries the reason a connection is unusable", () => {
    // Given a connection whose token mint was refused
    const live = aConnectionThatIs("connecting");
    cy.mount(<StatusProbe connection={live.connection} />);

    // When the refusal lands
    cy.then(() => live.becomes("error", "browser is not authorised for this room"));

    // Then the overlay has something to show beyond "failed"
    byTestId(STATUS_EL).should("have.text", "error");
    byTestId(ERROR_EL).should("have.text", "browser is not authorised for this room");
  });

  it("hands back the identical observation while the status holds", () => {
    // Given a settled connection, watched across many sampling intervals
    const live = aConnectionThatIs("connected");
    cy.mount(<StatusProbe connection={live.connection} />);
    byTestId(STATUS_EL).should("have.text", "connected");

    // When several intervals' worth of samples have been taken
    cy.wait(SEVERAL_SAMPLE_INTERVALS);

    // Then exactly one observation has ever been produced. A fresh object per sample would
    // re-render every attached session's runtime five times a second, for as long as the drawer is
    // open — and the terminals inside them with it
    byTestId(DISTINCT_EL).should("have.text", "1");
  });

  it("stops sampling once nobody is watching", () => {
    // Given a watched connection
    const live = aConnectionThatIs("connected");
    cy.mount(<DismissableStatusProbe connection={live.connection} />);
    byTestId(STATUS_EL).should("have.text", "connected");

    // When the watcher goes away
    byTestId("dismiss").click();
    byTestId(STATUS_EL).should("not.exist");

    // Then the connection stops being read. An interval left running is one per session the screen
    // ever showed, ticking until the tab is closed
    cy.then(() => cy.wrap(live.reads()).as("readsAtUnmount"));
    cy.wait(SEVERAL_SAMPLE_INTERVALS);
    cy.get<number>("@readsAtUnmount").then((atUnmount) => {
      expect(live.reads()).to.equal(atUnmount);
    });
  });

  it("reads a connection that does not exist as idle", () => {
    // Given a session with nothing attached yet
    cy.mount(<StatusProbe connection={null} />);

    // Then nothing has been asked of it — a different claim from something having failed, and the
    // difference between an empty pane and a pane saying the connection broke
    byTestId(STATUS_EL).should("have.text", "idle");
    byTestId(ERROR_EL).should("have.text", "none");
  });
});
