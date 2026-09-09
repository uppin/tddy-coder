import { TEST_IDS, byTestId } from "../testIds";

const ROW_TEST_ID_PREFIX = "hosts-row-";

/** Page object for the Hosts screen (`#/hosts`). */
export const hostsScreenPage = {
  row: (instanceId: string) => byTestId(`${ROW_TEST_ID_PREFIX}${instanceId}`),
  /**
   * The row elements, in the order the screen renders them.
   *
   * Selected structurally — every `<tr>` in the table body — rather than by a `hosts-row-` prefix
   * match. A prefix match would also collect each row's own cells, which share that prefix by
   * construction, and would need a new `:not()` clause for every column a later node adds.
   */
  rows: () => byTestId(TEST_IDS.hostsTable).find("tbody tr"),
  /**
   * The instance ids of the rendered rows, in render order.
   *
   * Built from queries only (`.find`, `.invoke`), so an assertion chained onto it is retried until
   * the screen settles rather than reading a half-rendered table once.
   */
  rowOrder: () =>
    hostsScreenPage
      .rows()
      .invoke("toArray")
      .invoke("map", (el: HTMLElement) =>
        (el.getAttribute("data-testid") ?? "").slice(ROW_TEST_ID_PREFIX.length),
      ),
  liveness: (instanceId: string) => byTestId(`${ROW_TEST_ID_PREFIX}${instanceId}-liveness`),
  lastSeen: (instanceId: string) => byTestId(`${ROW_TEST_ID_PREFIX}${instanceId}-last-seen`),
  /**
   * The "(local)" marker on a row — the only thing on the screen that says which host is serving
   * the page, since every daemon's own label already reads "… (this daemon)".
   *
   * Deliberately named outside the `hosts-row-` namespace; see `HostsScreen.tsx`.
   */
  localMarker: (instanceId: string) => byTestId(`hosts-local-marker-${instanceId}`),
  navEntry: () => byTestId(TEST_IDS.shellMenuHosts),
};

/**
 * Telemetry cell accessors — added by `#hosts-screen 2/8`.
 *
 * Every selector, `data-` attribute and presentation glyph the telemetry cell renders lives here,
 * so a test body reads as behaviour rather than as DOM. The `expect*` helpers exist for the same
 * reason: `data-core-{n}` is the cell's contract with this page object, not something a test should
 * spell out.
 */
export const hostTelemetryPage = {
  cell: (instanceId: string) => byTestId(`${ROW_TEST_ID_PREFIX}${instanceId}-telemetry`),
  cpu: (instanceId: string) => byTestId(`${ROW_TEST_ID_PREFIX}${instanceId}-cpu`),
  disk: (instanceId: string) => byTestId(`${ROW_TEST_ID_PREFIX}${instanceId}-disk`),
  unavailable: (instanceId: string) =>
    byTestId(`${ROW_TEST_ID_PREFIX}${instanceId}-telemetry-unavailable`),
  /** A reading arrived carrying disk but no CPU — the CPU slot alone is still waiting. */
  cpuPending: (instanceId: string) => byTestId(`${ROW_TEST_ID_PREFIX}${instanceId}-cpu-pending`),
  /** Subscribed, but the host has not reported a reading yet. */
  pending: (instanceId: string) =>
    byTestId(`${ROW_TEST_ID_PREFIX}${instanceId}-telemetry-pending`),
  /** Not in the roster — distinct from "reachable but silent", and never a number. */
  offline: (instanceId: string) =>
    byTestId(`${ROW_TEST_ID_PREFIX}${instanceId}-telemetry-offline`),

  /** Assert the per-core percentages the row is showing, core 0 first. */
  expectCpuCores: (instanceId: string, percents: number[]) => {
    percents.forEach((percent, index) => {
      hostTelemetryPage
        .cpu(instanceId)
        .should("have.attr", `data-core-${index}`, String(percent));
    });
  },

  /** Assert the free-disk figure the row is showing, as an operator reads it. */
  expectFreeDisk: (instanceId: string, formatted: string) =>
    hostTelemetryPage.disk(instanceId).should("contain.text", formatted),

  /** Assert the row is showing no reading at all — neither metric rendered. */
  expectNoReading: (instanceId: string) => {
    hostTelemetryPage.cpu(instanceId).should("not.exist");
    hostTelemetryPage.disk(instanceId).should("not.exist");
  },
};

/**
 * Tooling section selectors — added by `#hosts-screen 4/8`.
 *
 * As with the telemetry cell above, the DOM contract lives here rather than in test bodies. That
 * matters most for `expectGhLoginLabelledAsHosts`: the cell renders a static `gh` label in *every*
 * state, so asserting the cell's text contains "gh" proves nothing at all. What actually
 * distinguishes this host's `gh` login from the tddy session user in `UserAvatar` is the `title`,
 * and which attribute carries that is this page object's business, not a test's.
 */
export const hostToolingPage = {
  section: (instanceId: string) => byTestId(`${ROW_TEST_ID_PREFIX}${instanceId}-tooling`),
  gitIdentity: (instanceId: string) => byTestId(`${ROW_TEST_ID_PREFIX}${instanceId}-git`),
  githubCli: (instanceId: string) => byTestId(`${ROW_TEST_ID_PREFIX}${instanceId}-gh`),

  /** Assert the `gh` cell names the login as this *host's*, and says which login it is. */
  expectGhLoginLabelledAsHosts: (instanceId: string, login: string) => {
    hostToolingPage
      .githubCli(instanceId)
      .should("have.attr", "title")
      .and("match", /this host's/i);
    hostToolingPage.githubCli(instanceId).should("have.attr", "title").and("contain", login);
  },
};

/** ssh-agent section selectors — added by `#hosts-screen 5/8`. */
export const hostSshAgentPage = {
  section: (instanceId: string) => cy.get(`[data-testid="hosts-row-${instanceId}-ssh-agent"]`),
  keys: (instanceId: string) => cy.get(`[data-testid^="hosts-row-${instanceId}-ssh-key-"]`),
  key: (instanceId: string, fingerprint: string) =>
    cy.get(`[data-testid="hosts-row-${instanceId}-ssh-key-${fingerprint}"]`),
};
