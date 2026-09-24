/**
 * Page object for the redirect-flow GitHub sign-in callback, read through probes beside it that
 * render the shared auth context's view of the operator and the error it holds.
 *
 * All raw selectors live here; test bodies call named methods.
 */

import { byTestId, TEST_IDS } from "../testIds";

export const redirectLoginPage = {
  expectSignedInAs(login: string) {
    byTestId(TEST_IDS.authProbeStatus).should("have.text", `signed-in:${login}`);
  },

  expectSignedOut() {
    byTestId(TEST_IDS.authProbeStatus).should("have.text", "signed-out");
  },

  /** Sign-in ended on an error, and the error is exactly `message`. */
  expectSignInError(message: string) {
    byTestId(TEST_IDS.authProbeError).should("have.text", message);
  },

  expectNoSignInError() {
    byTestId(TEST_IDS.authProbeError).should("have.text", "");
  },
};
