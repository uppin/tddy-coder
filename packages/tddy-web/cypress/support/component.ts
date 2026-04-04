import { mount } from "cypress/react";

/** Default `strict: false` so React 18 dev double-mount does not discard imperative refs / font state mid-test. */
Cypress.Commands.add("mount", (jsx, options = {}) => {
  return mount(jsx, { strict: false, ...options });
});
