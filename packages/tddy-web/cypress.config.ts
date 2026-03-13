import { defineConfig } from "cypress";

export default defineConfig({
  component: {
    devServer: {
      framework: "react",
      bundler: "vite",
    },
    specPattern: "cypress/component/**/*.cy.{ts,tsx}",
    supportFile: "cypress/support/component.ts",
  },
  e2e: {
    baseUrl: "http://localhost:6006",
    specPattern: "cypress/e2e/**/*.cy.{ts,tsx}",
    supportFile: "cypress/support/e2e.ts",
  },
});
