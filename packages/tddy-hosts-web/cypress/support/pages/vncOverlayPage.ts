/** Page object for the VNC overlay. Raw selectors live here; spec bodies call named methods. */
import { byTestId, TEST_IDS } from "../testIds";

export const vncOverlayPage = {
  vncOverlay: (options?: Parameters<typeof cy.get>[1]) => byTestId(TEST_IDS.vncOverlay, { timeout: 5000, ...options }),
  vncOverlayClose: (options?: Parameters<typeof cy.get>[1]) => byTestId(TEST_IDS.vncOverlayClose, { timeout: 5000, ...options }),
  vncOverlayVideo: (options?: Parameters<typeof cy.get>[1]) => byTestId(TEST_IDS.vncOverlayVideo, { timeout: 5000, ...options }),
};
