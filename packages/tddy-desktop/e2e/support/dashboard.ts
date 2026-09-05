/**
 * Page object for the dashboard as the desktop application renders it.
 *
 * Specs say what an operator does; every selector and every `browser.execute` lives here.
 */

/** The dashboard, once the app's window is up. */
export function aDashboard() {
  const driver = {
    /** Wait until the daemon in this process has answered the page's first RPC. */
    async waitUntilLoaded() {
      // `Loading…` is shown until `GetClientConfig` settles over the webview IPC bridge, so a
      // rendered sign-in button is itself proof that the in-process daemon answered.
      await $("[data-testid='github-login-button']").waitForDisplayed({ timeout: 60_000 });
      return driver;
    },

    async clickSignIn() {
      await $("[data-testid='github-login-button']").click();
      return driver;
    },

    /**
     * The OAuth state the app stores immediately before navigating to the authorize URL. Present
     * means the daemon answered `GetAuthUrl` and the sign-in actually advanced.
     */
    oauthState() {
      return browser.execute(() => window.sessionStorage.getItem("tddy_oauth_state"));
    },

    /** Whatever error the auth flow surfaced, if any. */
    async visibleError() {
      const banner = await $("[data-testid='auth-error']");
      return (await banner.isExisting()) ? banner.getText() : null;
    },
  };
  return driver;
}
