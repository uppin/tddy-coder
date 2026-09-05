/**
 * Page object for the dashboard as the desktop application renders it.
 *
 * Specs say what an operator does; every selector and every `browser.execute` lives here.
 */

/**
 * Where the dashboard is served. The suite serves the built bundle here, and the app's own
 * `devUrl` is the same origin — Tauri grants capabilities per origin, so a page loaded from
 * anywhere else would have its IPC commands refused rather than merely unreachable.
 */
const DASHBOARD_URL = "http://localhost:5173/";

/** The dashboard, once the app's window is up. */
export function aDashboard() {
  const driver = {
    /**
     * Attach to the window the dashboard is in.
     *
     * The `embedded` provider drives a headless webview it owns inside the application process,
     * not the visible window, so the dashboard has to be loaded into it.
     */
    async attachToDashboard() {
      // The driver hands out a webview of its own, inside the application process, which starts
      // blank — so the dashboard is loaded into it rather than found. What makes this a real test
      // is that the webview is the app's: its IPC bridge reaches the daemon in this same process.
      await browser.url(DASHBOARD_URL);
      return driver;
    },

    /** Wait until the daemon in this process has answered the page's first RPC. */
    async waitUntilLoaded() {
      // `Loading…` is shown until `GetClientConfig` settles over the webview IPC bridge, so a
      // rendered sign-in button is itself proof that the in-process daemon answered. When it never
      // arrives, say what the window actually held — a blank page and a page stuck loading fail
      // identically otherwise, and they have different causes.
      await $("[data-testid='github-login-button']").waitForDisplayed({
        timeout: 60_000,
        timeoutMsg: `the dashboard never finished loading. ${await driver.describeWindow()}`,
      });
      return driver;
    },

    /** What the window is actually showing, for a failure message. */
    async describeWindow() {
      const [url, title, hasTauri, bodyText] = await Promise.all([
        browser.getUrl().catch((e) => `<getUrl failed: ${e}>`),
        browser.getTitle().catch(() => "<no title>"),
        browser.execute(() => Boolean((window as unknown as Record<string, unknown>).__TAURI_INTERNALS__)),
        browser.execute(() => document.body?.innerText?.slice(0, 300) ?? "<no body>"),
      ]);
      return `url=${url} title=${title} tauriBridge=${hasTauri} body=${JSON.stringify(bodyText)}`;
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
