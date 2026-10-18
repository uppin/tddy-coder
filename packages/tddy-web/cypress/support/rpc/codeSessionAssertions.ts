/**
 * Fluent assertions on what a `CodeIndexHost` saw of the pane's code sessions. Each one is retried
 * until the host has seen exactly what is expected (or the command times out).
 *
 *   expectTheHost(host).toHaveOpened([THE_WORKTREE]);
 *   expectTheHost(host).toHaveBeenAskedOfKind("PROBE", [{ kind: "PROBE", ...inMainRs(MAIN) }]);
 */

import type { AskedQuery, CodeIndexHost, OpenedWorktree } from "./codeSessionBackend";

export function expectTheHost(host: CodeIndexHost) {
  const retried = (assertion: () => void) => {
    cy.wrap(null).should(assertion);
    return expectations;
  };
  const expectations = {
    /** Exactly these `open` frames arrived, in order — one per session attempt. */
    toHaveOpened: (expected: OpenedWorktree[]) =>
      retried(() => expect(host.openedWorktrees()).to.deep.equal(expected)),

    /** Exactly these queries arrived, in any order. */
    toHaveBeenAsked: (expected: AskedQuery[]) =>
      retried(() => expect(host.queriesAsked()).to.have.deep.members(expected)),

    /** Exactly these queries of `kind` arrived, in order. */
    toHaveBeenAskedOfKind: (kind: AskedQuery["kind"], expected: AskedQuery[]) =>
      retried(() =>
        expect(host.queriesAsked().filter((query) => query.kind === kind)).to.deep.equal(expected),
      ),

    /** Exactly these queries were cancelled, in order. */
    toHaveCancelled: (expected: AskedQuery[]) =>
      retried(() => expect(host.queriesCancelled()).to.deep.equal(expected)),

    /** `open` sessions are open now and the pane has ended `ended`. */
    toHaveOpenAndEndedSessions: (open: number, ended: number) =>
      retried(() =>
        expect({ open: host.openSessionCount(), ended: host.endedSessionCount() }).to.deep.equal({
          open,
          ended,
        }),
      ),
  };
  return expectations;
}
