import "./commands";

// Clear app storage before each test (localStorage + sessionStorage).
beforeEach(() => {
  cy.clearAppStorage();
});
