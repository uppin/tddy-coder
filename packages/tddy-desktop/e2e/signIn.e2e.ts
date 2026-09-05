/**
 * End-to-end: the real application window, its webview-IPC transport, and `tddy-daemon` running in
 * the same process.
 *
 * Nothing below stubs anything. A passing run means a compiled Tauri binary assembled the daemon,
 * the page reached it over the IPC bridge, and the answers came back — the one claim no other test
 * in this repo can make.
 */

import { aDashboard } from "./support/dashboard.js";

describe("Tddy Desktop", () => {
  it("reaches the daemon running in its own process", async () => {
    // Given the application window
    const dashboard = aDashboard();

    // When it finishes loading
    await dashboard.attachToDashboard();
    await dashboard.waitUntilLoaded();

    // Then the page got its client configuration from the in-process daemon — a page that did not
    // would still be showing "Loading…"
    await expect($("[data-testid='github-login-button']")).toBeDisplayed();
  });

  it("asks the daemon for an authorize URL when sign-in is chosen", async () => {
    // Given a loaded dashboard
    const dashboard = aDashboard();
    await dashboard.attachToDashboard();
    await dashboard.waitUntilLoaded();

    // When the operator chooses to sign in
    await dashboard.clickSignIn();

    // Then the app stored the OAuth state it writes immediately before navigating, which it can
    // only have from a `GetAuthUrl` the daemon answered
    await browser.waitUntil(async () => (await dashboard.oauthState()) !== null, {
      timeout: 20_000,
      timeoutMsg: `sign-in never advanced: no OAuth state was stored (visible error: ${await dashboard.visibleError()})`,
    });
  });
});
