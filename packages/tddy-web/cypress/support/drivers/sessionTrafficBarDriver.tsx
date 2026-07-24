/**
 * Fluent component driver for the screen-level byte-traffic readout (`StatusBar` →
 * `SessionTrafficStrip`), exercised over a REAL `SessionRuntimeRegistry` + `makeByteTap` so the
 * test drives production aggregation (not hand-fed props).
 *
 * The harness subscribes to the registry exactly the way `SessionsDrawerScreen` does
 * (`useSyncExternalStore` → `runtimes`) and hands both `runtimes` and the registry to `StatusBar`,
 * so folding a session's byte tap re-renders the readout with the aggregate across every mounted
 * runtime — focused AND backgrounded.
 *
 * Usage:
 *
 *   aSessionTrafficBar()
 *     .withAttachedSession("focused", { focused: true })
 *     .withAttachedSession("background")
 *     .mount()
 *     .receiveBytes("focused", { bytesIn: 1_000 })
 *     .receiveBytes("background", { bytesIn: 2_000 })
 *     .expectBytesIn("3.0 kB");
 */

import React, { useSyncExternalStore } from "react";
import { mount } from "cypress/react";
import { StatusBar } from "../../../src/components/sessions/StatusBar";
import {
  SessionRuntimeRegistry,
  makeByteTap,
  type ByteDelta,
} from "../../../src/components/sessions/sessionRuntimeRegistry";
import type { SessionAttachmentState } from "../../../src/components/sessions/useSessionAttachment";
import { byTestId, TEST_IDS } from "../testIds";

const IDLE_ATTACHMENT: SessionAttachmentState = { status: "idle" };

function TrafficBarHarness({ registry }: { registry: SessionRuntimeRegistry }) {
  const runtimes = useSyncExternalStore(
    (listener) => registry.subscribe(listener),
    () => registry.runtimes,
    () => registry.runtimes,
  );
  return <StatusBar attachment={IDLE_ATTACHMENT} runtimes={runtimes} runtimeRegistry={registry} />;
}

export function aSessionTrafficBar() {
  const registry = new SessionRuntimeRegistry();

  const api = {
    /** Register an attached runtime (as if a session were attached and mounted). The first one
     *  registered becomes focused unless a later call passes `{ focused: true }`. */
    withAttachedSession(
      sessionId: string,
      opts: { focused?: boolean; bytesIn?: number; bytesOut?: number } = {},
    ) {
      registry.add(sessionId, {
        sessionId,
        attached: true,
        status: "connected-livekit",
        livekitUrl: "ws://127.0.0.1:7880",
        livekitRoom: `room-${sessionId}`,
        livekitServerIdentity: `daemon-local-${sessionId}`,
        identity: `browser-${sessionId}`,
        bytesIn: opts.bytesIn ?? 0,
        bytesOut: opts.bytesOut ?? 0,
        lastDataReceivedAt: null,
      });
      if (opts.focused) registry.focus(sessionId);
      return api;
    },
    mount() {
      mount(<TrafficBarHarness registry={registry} />);
      return api;
    },
    /** Fold a terminal I/O event into a session's runtime via the production byte tap. */
    receiveBytes(sessionId: string, delta: ByteDelta) {
      cy.then(() => makeByteTap(registry, sessionId)(delta));
      return api;
    },
    /** Evict a session's runtime (explicit disconnect). */
    disconnect(sessionId: string) {
      cy.then(() => registry.disconnect(sessionId));
      return api;
    },
    expectStripVisible() {
      byTestId(TEST_IDS.sessionTrafficStrip).should("exist");
      return api;
    },
    expectBytesIn(text: string) {
      byTestId(TEST_IDS.sessionTrafficBytesIn).should("contain.text", text);
      return api;
    },
    expectBytesOut(text: string) {
      byTestId(TEST_IDS.sessionTrafficBytesOut).should("contain.text", text);
      return api;
    },
  };

  return api;
}
