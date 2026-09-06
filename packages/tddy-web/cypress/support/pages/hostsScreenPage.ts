/** Page object for the Hosts screen (`#/hosts`). */
export const hostsScreenPage = {
  screen: () => cy.get('[data-testid="hosts-screen"]'),
  row: (instanceId: string) => cy.get(`[data-testid="hosts-row-${instanceId}"]`),
  rows: () => cy.get('[data-testid^="hosts-row-"]'),
  liveness: (instanceId: string) =>
    cy.get(`[data-testid="hosts-row-${instanceId}-liveness"]`),
  lastSeen: (instanceId: string) =>
    cy.get(`[data-testid="hosts-row-${instanceId}-last-seen"]`),
  navEntry: () => cy.get('[data-testid="shell-menu-hosts"]'),
};
