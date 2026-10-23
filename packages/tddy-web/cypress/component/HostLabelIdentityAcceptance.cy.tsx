/**
 * `useHostLabel`'s identity contract — what keeps a render-time label current under the React
 * Compiler (which memoises a call on its inputs) without restarting every poll keyed on it.
 *
 * - A host's name changing gives a new callback, so a compiled caller recomputes its label.
 * - The daemon list merely being rebuilt with the same names (every participant event does that)
 *   keeps the callback, so effects keyed on it are not re-run.
 *
 * Component tests run uncompiled, so these pin the identity rule rather than the compiler itself.
 */

import React, { useRef, useState } from "react";
import { Room } from "livekit-client";
import type { DaemonHost } from "../../src/lib/participantRole";
import { SelectedDaemonProvider, useHostLabel } from "../../src/rpc/selectedDaemon";
import { AuthProvider } from "../../src/hooks/authProvider";
import { byTestId } from "../support/testIds";

const HOST = "workstation-1";

function LabelProbe() {
  const hostLabel = useHostLabel();
  const seen = useRef(new Set<unknown>());
  seen.current.add(hostLabel);
  return (
    <div>
      <span data-testid="probe-label">{hostLabel(HOST)}</span>
      <span data-testid="probe-identities">{seen.current.size}</span>
    </div>
  );
}

function Harness() {
  const [daemons, setDaemons] = useState<DaemonHost[]>([{ instanceId: HOST, label: HOST }]);
  return (
    <AuthProvider>
      <button data-testid="rebuild-same" onClick={() => setDaemons((d) => d.map((x) => ({ ...x })))}>
        rebuild
      </button>
      <button
        data-testid="rename"
        onClick={() => setDaemons([{ instanceId: HOST, label: "build box (this daemon)" }])}
      >
        rename
      </button>
      <SelectedDaemonProvider room={new Room()} daemons={daemons} servingInstanceId={HOST}>
        <LabelProbe />
      </SelectedDaemonProvider>
    </AuthProvider>
  );
}

beforeEach(() => {
  cy.clearAllSessionStorage();
  window.localStorage.setItem("tddy_session_token", "fake-token");
});

it("keeps the same callback when the daemon list is rebuilt with the same names", () => {
  // Given
  cy.mount(<Harness />);
  byTestId("probe-label").should("have.text", HOST);

  // When the list is rebuilt, as every participant event does
  byTestId("rebuild-same").click();
  byTestId("rebuild-same").click();

  // Then
  byTestId("probe-identities").should("have.text", "1");
});

it("gives a new callback, and the new name, when a host's label changes", () => {
  // Given
  cy.mount(<Harness />);
  byTestId("probe-label").should("have.text", HOST);

  // When
  byTestId("rename").click();

  // Then the self-advertised suffix is dropped, by the one naming rule
  byTestId("probe-label").should("have.text", "build box");
  byTestId("probe-identities").should("have.text", "2");
});
