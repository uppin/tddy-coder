/**
 * Page object for the device-flow GitHub sign-in (`DeviceLoginPanel`) and the sign-in screen that
 * chooses between it and the redirect-flow button (`DaemonLoginScreen`).
 *
 * All raw selectors live here; test bodies call named methods.
 */

import { byTestId, TEST_IDS } from "../testIds";

export const deviceLoginPage = {
  /** Ask the daemon for a device code — also how an ended attempt is started again. */
  startSignIn() {
    byTestId(TEST_IDS.deviceLoginStart).click();
  },

  expectUserCode(userCode: string) {
    byTestId(TEST_IDS.deviceLoginUserCode).should("be.visible").and("have.text", userCode);
  },

  expectNoUserCode() {
    byTestId(TEST_IDS.deviceLoginUserCode).should("not.exist");
  },

  /** The verification page is a real link the operator can follow, opened beside this page. */
  expectVerificationLinkTo(uri: string) {
    byTestId(TEST_IDS.deviceLoginVerificationLink)
      .should("be.visible")
      .and("have.attr", "href", uri)
      .and("have.attr", "target", "_blank");
  },

  expectOfferedToStartAgain() {
    byTestId(TEST_IDS.deviceLoginStart).should("be.visible").and("be.enabled");
  },

  expectDeniedMessage() {
    byTestId(TEST_IDS.deviceLoginDenied).should("be.visible").and("contain.text", "denied");
  },

  expectNoDeniedMessage() {
    byTestId(TEST_IDS.deviceLoginDenied).should("not.exist");
  },

  expectExpiredMessage() {
    byTestId(TEST_IDS.deviceLoginExpired).should("be.visible").and("contain.text", "expired");
  },

  expectNoExpiredMessage() {
    byTestId(TEST_IDS.deviceLoginExpired).should("not.exist");
  },

  /** The attempt ended on an error rather than on GitHub's answer, and the error says `containing`. */
  expectFailedMessage(containing: string) {
    byTestId(TEST_IDS.deviceLoginError).should("be.visible").and("contain.text", containing);
  },

  /** The daemon declared no sign-in flow: the screen says so rather than offering one. */
  expectNoSignInConfigured() {
    byTestId(TEST_IDS.daemonLoginNoSignIn)
      .should("be.visible")
      .and("contain.text", "no GitHub sign-in configured");
  },

  /** The daemon declared a flow this page does not know, and the screen names it. */
  expectUnrecognisedFlowNamed(flow: string) {
    byTestId(TEST_IDS.daemonLoginUnrecognisedFlow).should("be.visible").and("contain.text", flow);
  },

  expectDeviceFlowOffered() {
    byTestId(TEST_IDS.deviceLoginStart).should("be.visible");
  },

  expectNoDeviceFlow() {
    byTestId(TEST_IDS.deviceLoginStart).should("not.exist");
  },

  expectRedirectButtonOffered() {
    byTestId(TEST_IDS.githubLoginButton).should("be.visible");
  },

  expectNoRedirectButton() {
    byTestId(TEST_IDS.githubLoginButton).should("not.exist");
  },

  // ---------------------------------------------------------------------------
  // The shared auth context, as a probe beside the panel renders it
  // ---------------------------------------------------------------------------

  expectSignedInAs(login: string) {
    byTestId(TEST_IDS.authProbeStatus).should("have.text", `signed-in:${login}`);
  },

  expectSignedOut() {
    byTestId(TEST_IDS.authProbeStatus).should("have.text", "signed-out");
  },
};
