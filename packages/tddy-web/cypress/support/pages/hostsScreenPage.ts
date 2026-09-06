/** Page object for the Hosts screen (`#/hosts`). */
export const hostsScreenPage = {
  screen: () => cy.get('[data-testid="hosts-screen"]'),
  row: (instanceId: string) => cy.get(`[data-testid="hosts-row-${instanceId}"]`),
  /**
   * The row elements only. A bare `^=` prefix match would also collect each row's own
   * `-liveness` and `-last-seen` children, which share the prefix by construction.
   */
  rows: () =>
    cy.get(
      '[data-testid^="hosts-row-"]:not([data-testid$="-liveness"]):not([data-testid$="-last-seen"])',
    ),
  liveness: (instanceId: string) =>
    cy.get(`[data-testid="hosts-row-${instanceId}-liveness"]`),
  lastSeen: (instanceId: string) =>
    cy.get(`[data-testid="hosts-row-${instanceId}-last-seen"]`),
  navEntry: () => cy.get('[data-testid="shell-menu-hosts"]'),
};
