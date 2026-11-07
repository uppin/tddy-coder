/**
 * Acceptance tests: the session header says what the host is doing while the session's code index
 * warms.
 *
 * - The session header follows `code_navigation.WatchCodeIndex` and shows "Indexing — <phase> <n>%"
 *   until the index is ready, then nothing.
 *
 * Mid-stream state is asserted while the stub generator is held at a gate, so the rendered value is
 * exact rather than whatever the race settled on. (Split out of `SessionStartAndIndexingProgress.cy.tsx`,
 * whose create-pane half moved to `tddy-session-create-web`.)
 *
 * Feature docs: docs/ft/web/session-code-pane.md (Indexing indicator)
 */

import React from "react";
import { createClient } from "@connectrpc/connect";
import { create } from "@bufbuild/protobuf";
import { anInMemoryRpcBackend, type InMemoryRpcBackend } from "tddy-connectrpc-testkit";
import { CodeIndexProgressSchema, CodeNavigationService } from "tddy-client-web/gen/code_navigation_pb";
import { SessionIndexingIndicator } from "../../src/code/SessionIndexingIndicator";
import { mountWithRpc } from "../support/rpc/inMemory";
import { sessionIndexingIndicatorPage } from "../support/pages/sessionIndexingIndicatorPage";

const THE_SESSION = "session-indexing-1";

/** A gate a stub generator awaits, so the UI's mid-stream state is settled when it is asserted. */
interface Gate {
  held: Promise<void>;
  release: () => void;
}

function aGate(): Gate {
  let release: () => void = () => undefined;
  const held = new Promise<void>((resolve) => {
    release = resolve;
  });
  return { held, release: () => release() };
}

/**
 * A host whose warm of the session's code index reports 40% of its "Fetching" phase, holds until
 * `gate` is released, then reports the index ready and ends.
 */
function aHostWarmingTheSessionsIndex(gate: Gate): InMemoryRpcBackend {
  return anInMemoryRpcBackend().implement(CodeNavigationService, {
    async *watchCodeIndex() {
      yield create(CodeIndexProgressSchema, {
        line: "Fetching 40%",
        phase: "Fetching",
        percentage: 40,
        furthest: "40%",
      });
      await gate.held;
      yield create(CodeIndexProgressSchema, {
        line: "ready",
        phase: "Fetching",
        percentage: 100,
        furthest: "100%",
        ready: true,
      });
    },
  });
}

function mountTheIndexingIndicator(backend: InMemoryRpcBackend) {
  mountWithRpc(
    <SessionIndexingIndicator
      client={createClient(CodeNavigationService, backend.transport())}
      sessionToken="fake-token"
      sessionId={THE_SESSION}
    />,
    backend,
  );
}

beforeEach(() => {
  cy.viewport(1280, 900);
  cy.clearAllSessionStorage();
});

it("the session header shows indexing until ready", () => {
  // Given a host warming the session's code index, 40% through its "Fetching" phase
  const gate = aGate();
  mountTheIndexingIndicator(aHostWarmingTheSessionsIndex(gate));

  // Then the header says the index is loading, with its phase and percentage
  sessionIndexingIndicatorPage.indicator().should("have.text", "Indexing — Fetching 40%");

  // When the index becomes ready
  cy.then(() => gate.release());

  // Then the indicator is gone
  sessionIndexingIndicatorPage.indicator().should("not.exist");
});
