import { defineConfig } from "@playwright/test";

export default defineConfig({
  testDir: "playwright",
  // `*.pw.ts` — Bun's test runner treats `*.spec.ts` / `*.test.ts` as unit tests; Playwright must use a distinct suffix.
  testMatch: "**/*.pw.ts",
  timeout: 60_000,
  expect: { timeout: 20_000 },
  use: {
    trace: "retain-on-failure",
    launchOptions: {
      args: [
        "--no-sandbox",
        "--disable-gpu",
        "--allow-insecure-localhost",
        "--disable-web-security",
        "--disable-features=PrivateNetworkAccessSendPreflights,PrivateNetworkAccessRespectPreflightResults,BlockInsecurePrivateNetworkRequests",
      ],
    },
  },
  projects: [
    {
      name: "chromium",
      use: { browserName: "chromium" },
    },
  ],
});
