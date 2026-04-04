import { visibleSessionTableHeaderTestIdsForWidth } from "../../src/components/connection/sessionTableColumns";

describe("ConnectionSessionTablesSection responsive columns (story layout)", () => {
  it("narrow host hides model column while id status and actions stay visible", () => {
    cy.mountSessionTablesDemo({ outerWidthPx: 360 });
    cy.get('[data-testid="sessions-table-proj-1"]').should("exist");
    cy.get('[data-testid="session-table-col-header-model"]').should("not.be.visible");
    cy.get('[data-testid="session-table-col-header-id"]').should("be.visible");
    cy.get('[data-testid="session-table-col-header-status"]').should("be.visible");
    cy.get('[data-testid="session-table-col-header-actions"]').should("be.visible");
  });

  it("visible project table headers match column policy for measured host width", () => {
    cy.mountSessionTablesDemo({ outerWidthPx: 720, viewport: [1440, 900] });
    cy.get('[data-testid="session-tables-layout-host"]').then(($host) => {
      const w = $host[0].getBoundingClientRect().width;
      const expected = visibleSessionTableHeaderTestIdsForWidth(w);
      cy.get('[data-testid="sessions-table-proj-1"] thead tr th')
        .filter(":visible")
        .then(($ths) => {
          const got = [...$ths].map((el) => el.getAttribute("data-testid") ?? "");
          expect(got).to.deep.equal(expected);
        });
    });
  });

  it("wide host shows full header row including model", () => {
    cy.mountSessionTablesDemo({ outerWidthPx: 1200, viewport: [1600, 900] });
    cy.get('[data-testid="sessions-table-proj-1"]', { timeout: 8000 }).should("exist");
    cy.get('[data-testid="session-table-col-header-model"]').should("be.visible");
    cy.get('[data-testid="session-table-col-header-agent"]').should("be.visible");
  });

  it("project and orphan tables expose the same visible headers at the same host width", () => {
    cy.mountSessionTablesDemo({ outerWidthPx: 520, viewport: [1000, 800] });
    cy.get('[data-testid="sessions-table-proj-1"]', { timeout: 8000 }).should("exist");
    cy.get('[data-testid="sessions-table-orphan"]').should("exist");
    const visibleHeaderIds = (tableSelector: string) =>
      cy.get(tableSelector).then(($table) => {
        const ids = $table
          .find(`thead [data-testid^="session-table-col-header-"]`)
          .filter((_i, el) => Cypress.$(el).is(":visible"))
          .map((_i, el) => el.getAttribute("data-testid") ?? "")
          .get();
        return ids;
      });
    visibleHeaderIds('[data-testid="sessions-table-proj-1"]').then((proj) => {
      visibleHeaderIds('[data-testid="sessions-table-orphan"]').then((orph) => {
        expect(proj.length).to.be.greaterThan(0);
        expect(orph).to.deep.equal(proj);
      });
    });
  });
});
