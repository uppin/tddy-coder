/**
 * Acceptance: the session terminal re-fits when the window collapses around it.
 *
 * **The load-bearing behavior is the re-fit on collapse, not the initial fit.** After a display
 * disconnect pulls the window onto a smaller screen, the pane can end up narrower than the canvas'
 * fitted width — a stuck flex chain (an item whose automatic minimum size is the canvas' width)
 * never resizes, the FitAddon's ResizeObserver sees no change, and the grid goes stale, clipping
 * text at the old edge. The real pane chain, from the host this mounts, must let the pane shrink
 * and re-fit the grid.
 */

import React, { useState } from "react";
import { anInMemoryRpcBackend } from "tddy-connectrpc-testkit";
// The real chain's shrink behavior lives in Tailwind utility classes (min-w-0, overflow-hidden) —
// without the app stylesheet they are inert and the test pins a floor that production does not have.
import "../../src/index.css";
import { SessionMainPane } from "../../src/components/sessions/SessionMainPane";
import type { SessionAttachmentState } from "../../src/components/sessions/useSessionAttachment";
import { aSessionConnection } from "../support/rpc/sessionConnections";
import { byTestId, TEST_IDS } from "../support/testIds";

const FAKE_SESSION = {
  sessionId: "livekit-terminal-test-aaaa-0000-0000-000000000001",
  createdAt: "2026-06-30T10:00:00Z",
  status: "active",
  repoPath: "/home/dev/my-recipe-session",
  pid: 43001,
  isActive: true,
  projectId: "proj-livekit-terminal-1",
  daemonInstanceId: "",
  workflowGoal: "plan-pr-stack recipe session",
  pendingElicitation: false,
};

const SESSION_TOKEN = "an-operator-access-token";

/** The real pane chain inside a width-controlled host, shrinkable mid-test. */
function ShrinkableHost() {
  const [width, setWidth] = useState(1200);
  const connection = React.useMemo(
    () =>
      aSessionConnection(FAKE_SESSION.sessionId)
        .carriedByRoom("tddy-lobby", {
          url: "ws://localhost:9999",
          serverIdentity: "daemon-dev-livekit-terminal-test-0001",
        })
        .servingOver(anInMemoryRpcBackend().transport())
        .servingTerminal()
        .build(),
    [],
  );
  const hint = React.useMemo(
    () =>
      aSessionConnection(FAKE_SESSION.sessionId)
        .carriedByRoom("tddy-lobby", {
          url: "ws://localhost:9999",
          serverIdentity: "daemon-dev-livekit-terminal-test-0001",
        })
        .servingOver(anInMemoryRpcBackend().transport())
        .servingTerminal()
        .buildHint(),
    [],
  );
  const attachment: SessionAttachmentState = { status: "connected", connection };
  return (
    <div style={{ display: "flex", flexDirection: "column", height: 600 }}>
      <div style={{ display: "flex", width, flex: 1, minHeight: 0 }}>
        <SessionMainPane
          host={null}
          selectedSession={FAKE_SESSION as never}
          attachment={attachment}
          attachmentHint={hint}
          inspectorState="closed"
          onToggleInspector={cy.stub()}
          onInspectorClose={cy.stub()}
          onInspectorExpand={cy.stub()}
          onInspectorRestore={cy.stub()}
          onResume={cy.stub()}
          onDelete={cy.stub()}
          onTerminate={cy.stub()}
          sessionToken={SESSION_TOKEN}
          runtimes={[
            {
              sessionId: FAKE_SESSION.sessionId,
              attached: true,
              connection,
              hint,
              bytesIn: 0,
              bytesOut: 0,
              lastDataReceivedAt: null,
            },
          ]}
          focusedRuntimeId={FAKE_SESSION.sessionId}
        />
      </div>
      <button
        type="button"
        data-testid="shrink-host"
        onClick={() => setWidth(300)}
        style={{ position: "fixed", bottom: 4, right: 4, zIndex: 50 }}
      >
        shrink
      </button>
    </div>
  );
}

describe("GhosttyTerminal shrink — the real pane chain", () => {
  it("re-fits the grid when the pane collapses below the canvas' fitted width", () => {
    // Given — a wide pane with the real terminal chain mounted
    cy.mount(<ShrinkableHost />);
    byTestId(TEST_IDS.ghosttyTerminal, { timeout: 10000 }).should(
      "have.attr",
      "data-terminal-ready",
      "true",
    );
    byTestId(TEST_IDS.ghosttyTerminal)
      .find("canvas")
      .then(($canvas) => {
        const fittedWidth = parseFloat($canvas[0].style.width);
        expect(fittedWidth).to.be.greaterThan(
          600,
          "terminal should have fitted to the wide pane",
        );

        // When — the window collapses to a quarter of its width
        cy.get("[data-testid='shrink-host']").click();

        // Then — the grid re-fits to the shrunken pane (the FitAddon's resize observer is
        // debounced at 100ms; on a loaded CI runner the wasm render loop can lag behind, so
        // give the re-fit a generous window — the timeout flows from .find into .should)
        byTestId(TEST_IDS.ghosttyTerminal)
          .find("canvas", { timeout: 15000 })
          .should(($c) => {
            const width = parseFloat($c[0].style.width);
            if (width < 300) return;

            // Diagnostic: CI has failed this twice with the canvas frozen at the fitted
            // width while the identical tree passed elsewhere. Distinguish the two
            // candidate mechanisms in the failure message itself (no artifacts are
            // uploaded): a container still ~fitted wide means the flex chain is stuck at
            // min-content; a shrunken container under an unfitted canvas means the
            // observer/fit path never ran.
            const chain: string[] = [];
            let node: HTMLElement | null = $c[0] as HTMLElement;
            while (node && chain.length < 16) {
              const cs = node.ownerDocument.defaultView!.getComputedStyle(node);
              const r = node.getBoundingClientRect();
              const cls =
                typeof node.className === "string"
                  ? node.className.trim().split(/\s+/).slice(0, 5).join(" ")
                  : "";
              chain.push(
                `${node.tagName.toLowerCase()}[${cls}] w=${Math.round(r.width)} ` +
                  `minW=${cs.minWidth} ovfX=${cs.overflowX} disp=${cs.display} pos=${cs.position}`,
              );
              node = node.parentElement;
            }
            throw new Error(
              `canvas did not re-fit (fitted ${fittedWidth}px, still ${width}px after 15s). ` +
                `Chain canvas→root:\n${chain.join("\n")}`,
            );
          });
      });
  });
});
