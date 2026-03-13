import { mount } from "cypress/react";

Cypress.Commands.add("mount", mount);

beforeEach(() => {
  cy.window({ log: false }).then((win) => {
    (win as unknown as { cypressLogs: string[] }).cypressLogs = [];
    const originalLog = (win as Window).console.log;
    (win as Window).console.log = function (...args: unknown[]) {
      const message = args
        .map((a) => (typeof a === "object" ? JSON.stringify(a) : String(a)))
        .join(" ");
      if (message.includes("[LiveKitTransport]") || message.includes("[TEST]")) {
        (win as unknown as { cypressLogs: string[] }).cypressLogs.push(message);
      }
      originalLog.apply((win as Window).console, args);
    };
  });
});

afterEach(() => {
  cy.window({ log: false }).then((win) => {
    const logs = (win as unknown as { cypressLogs?: string[] }).cypressLogs ?? [];
    logs.forEach((log: string) => {
      cy.task("log", `[BROWSER] ${log}`, { log: false });
    });
  });
});
