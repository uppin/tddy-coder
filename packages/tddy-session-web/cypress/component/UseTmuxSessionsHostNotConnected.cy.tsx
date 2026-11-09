/**
 * Cypress component spec: the tmux listing of a session whose host never connects.
 *
 * `useTmuxSessions` lists through the owning daemon's terminal client. When that client never
 * arrives, the "+" menu must say the host is not connected (the same words the terminal tabs use)
 * instead of showing "Loading tmux sessions…" for ever.
 *
 * Feature: docs/ft/web/session-terminal-tabs.md (`#session-tabs 3/7`)
 */

import React, { useEffect } from "react";
import { mount } from "cypress/react";
import { hostNotConnectedText } from "tddy-client-web";
import { useTmuxSessions } from "../../src/useTmuxSessions";
import type { TerminalClient } from "../../src/sessionRuntimeProps";

/** Renders the hook's state as text, and asks for a listing once on mount like the opening "+" menu. */
function TmuxMenuProbe({ client }: { client: TerminalClient | null }) {
  const tmux = useTmuxSessions({ sessionId: "s1", sessionToken: "t", client });
  useEffect(() => tmux.refresh(), [tmux.refresh]);
  return (
    <div>
      <p data-testid="probe-loading">{String(tmux.loading)}</p>
      <p data-testid="probe-error">{tmux.error ?? ""}</p>
      <p data-testid="probe-sessions">{tmux.sessions.map((s) => s.name).join(",")}</p>
    </div>
  );
}

/** The probe with no client until its button is clicked. */
function ClientArrivesOnClick({ client }: { client: TerminalClient }) {
  const [arrived, setArrived] = React.useState(false);
  return (
    <>
      <button type="button" data-testid="client-arrives" onClick={() => setArrived(true)}>
        arrive
      </button>
      <TmuxMenuProbe client={arrived ? client : null} />
    </>
  );
}

describe("The tmux listing of a session whose host is not connected", () => {
  it("says the host is not connected instead of loading for ever", () => {
    // Given a session whose terminal client never arrives
    // When the menu asks for the tmux listing
    mount(<TmuxMenuProbe client={null} />);

    // Then it reports the host as not connected, and is not loading
    cy.get('[data-testid="probe-error"]').should("have.text", hostNotConnectedText(null));
    cy.get('[data-testid="probe-loading"]').should("have.text", "false");
  });

  it("lists once the client arrives after having reported the host as not connected", () => {
    // Given a listing asked for while there was no client
    const client = {
      listTmuxSessions: async () => ({ sessions: [{ name: "alpha" }] }),
    } as unknown as TerminalClient;
    mount(<ClientArrivesOnClick client={client} />);
    cy.get('[data-testid="probe-error"]').should("have.text", hostNotConnectedText(null));

    // When the client arrives
    cy.get('[data-testid="client-arrives"]').click();

    // Then the sessions are listed and the error is gone
    cy.get('[data-testid="probe-sessions"]').should("have.text", "alpha");
    cy.get('[data-testid="probe-error"]').should("have.text", "");
  });
});
