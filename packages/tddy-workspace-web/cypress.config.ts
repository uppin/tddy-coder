import { defineConfig } from "cypress";

/** Read by `vite.config.ts` when Cypress spawns the component-testing dev server. */
process.env.CYPRESS_DISABLE_REACT_COMPILER = "1";

export default defineConfig({
  component: {
    devServer: { framework: "react", bundler: "vite" },
    specPattern: "cypress/component/**/*.cy.{ts,tsx}",
    supportFile: "cypress/support/component.ts",
    indexHtmlFile: "cypress/support/component-index.html",
  },
});
