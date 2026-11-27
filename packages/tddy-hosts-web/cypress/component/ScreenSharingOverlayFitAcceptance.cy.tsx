/**
 * Acceptance tests: a shared screen is drawn as large as the viewer allows, at its own aspect.
 *
 * A phone-sized portrait stream (an iOS Simulator) is far smaller than the window it is watched in;
 * drawn at its intrinsic size it is a small picture in a black screen. Its box is the picture
 * itself, so input scaled through that box lands on the device pixel the operator aimed at.
 */

// Layout is the point here, so the app stylesheet is loaded (see `cypress/support/component.ts`).
import "tddy-ui-web/theme.css";
import { aMediaDouble } from "tddy-client-web/testkit";
import { MediaPortProvider } from "tddy-client-web";
import { ScreenSharingOverlay } from "../../src/ScreenSharingOverlay";

/** An iPhone's device screen as the bridge reports it: 594×1292 pixels, portrait. */
const A_PHONE = { width: 594, height: 1292 };
const THE_VIEWER = { width: 1200, height: 800 };

function theBoxOf(testId: string) {
  return cy.get(`[data-testid="${testId}"]`).then(($el) => $el[0].getBoundingClientRect());
}

function mountAPhoneStream() {
  cy.viewport(THE_VIEWER.width, THE_VIEWER.height);
  cy.mount(
    <ScreenSharingOverlay
      room={null}
      bridgeIdentity="bridge-iphone"
      trackName="screenshare:iphone"
      width={A_PHONE.width}
      height={A_PHONE.height}
      onClose={() => undefined}
    />,
  );
}

describe("a small portrait stream", () => {
  it("fills the viewer's height", () => {
    // Given / When
    mountAPhoneStream();

    // Then
    theBoxOf("screen-sharing-overlay-video").should((box) => {
      expect(Math.round(box.height), "as tall as the viewer").to.equal(THE_VIEWER.height);
    });
  });

  it("keeps the device's aspect, so the box is exactly the picture", () => {
    // Given / When
    mountAPhoneStream();

    // Then
    theBoxOf("screen-sharing-overlay-video").should((box) => {
      const expectedWidth = (THE_VIEWER.height * A_PHONE.width) / A_PHONE.height;
      expect(box.width, "no wider than the device's aspect allows").to.be.closeTo(expectedWidth, 1);
    });
  });
});

describe("how the stream is arriving", () => {
  function mountAStreamArriving(stats: { kbps: number; width: number; height: number; fps: number } | null) {
    const media = aMediaDouble();
    if (stats) media.receiveVideoAt("bridge-iphone", "screenshare:iphone", stats);
    cy.mount(
      <MediaPortProvider port={media}>
        <ScreenSharingOverlay
          room={null}
          bridgeIdentity="bridge-iphone"
          trackName="screenshare:iphone"
          width={A_PHONE.width}
          height={A_PHONE.height}
          onClose={() => undefined}
        />
      </MediaPortProvider>,
    );
  }

  it("shows the bitrate, decoded size and frame rate", () => {
    // Given / When
    mountAStreamArriving({ kbps: 1850, width: 594, height: 1292, fps: 30 });

    // Then
    cy.get('[data-testid="screen-sharing-overlay-stats"]').should(
      "have.text",
      "1850 kbps · 594×1292 · 30 fps",
    );
  });

  it("shows nothing before a first interval is measured", () => {
    // Given / When
    mountAStreamArriving(null);

    // Then
    cy.get('[data-testid="screen-sharing-overlay-video"]').should("exist");
    cy.get('[data-testid="screen-sharing-overlay-stats"]').should("not.exist");
  });
});
