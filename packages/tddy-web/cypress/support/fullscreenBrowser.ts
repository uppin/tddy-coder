/**
 * A browser whose Fullscreen and Keyboard Lock APIs are fakes, installed on the AUT window.
 *
 * A real `requestFullscreen` needs a user activation the runner cannot provide, and a real
 * `navigator.keyboard.lock()` is only honoured while actually full screen — so neither can be
 * exercised for real. What these fakes pin is what the page **asked the browser to do**, and they
 * play the browser's part back: `document.fullscreenElement` follows the fake, and every change is
 * announced with `fullscreenchange`, which is the only signal a page gets that the browser left full
 * screen on its own (Escape, a system gesture).
 *
 * Aliases a spec can assert on: `@requestFullscreen`, `@exitFullscreen`, and — on a browser that
 * has Keyboard Lock — `@keyboardLock`, `@keyboardUnlock`.
 */

/** Make `el` the document's fullscreen element and announce it, as the browser does. */
function setFullscreenElement(win: Cypress.AUTWindow, el: Element | null): void {
  Object.defineProperty(win.document, "fullscreenElement", {
    configurable: true,
    get: () => el,
  });
  win.document.dispatchEvent(new win.Event("fullscreenchange"));
}

function installFullscreenApi(win: Cypress.AUTWindow): void {
  cy.stub(win.Element.prototype, "requestFullscreen")
    .as("requestFullscreen")
    .callsFake(function (this: Element) {
      setFullscreenElement(win, this);
      return Promise.resolve();
    });

  cy.stub(win.document, "exitFullscreen")
    .as("exitFullscreen")
    .callsFake(() => {
      setFullscreenElement(win, null);
      return Promise.resolve();
    });

  setFullscreenElement(win, null);
}

/**
 * Take back what an earlier test installed that `cy.stub` does not restore on its own.
 *
 * Component specs share one AUT window, so an own `navigator.keyboard` or `fullscreenElement` left
 * behind would make the next test's browser whatever this one's was.
 */
export function forgetTheFakeBrowser(): void {
  cy.window().then((win) => {
    Reflect.deleteProperty(win.navigator, "keyboard");
    Reflect.deleteProperty(win.document, "fullscreenElement");
  });
}

/** A browser that can go full screen and can lock the keyboard while it is. */
export function givenABrowserThatCanLockTheKeyboard(): void {
  cy.window().then((win) => {
    installFullscreenApi(win);
    Object.defineProperty(win.navigator, "keyboard", {
      configurable: true,
      value: {
        lock: cy.stub().as("keyboardLock").resolves(),
        unlock: cy.stub().as("keyboardUnlock"),
      },
    });
  });
}

/** A browser that can go full screen but has no Keyboard Lock API at all (Firefox, Safari). */
export function givenABrowserWithoutKeyboardLock(): void {
  cy.window().then((win) => {
    installFullscreenApi(win);
    Object.defineProperty(win.navigator, "keyboard", {
      configurable: true,
      value: undefined,
    });
  });
}

/**
 * The browser leaves full screen by itself — what pressing Escape does where the keyboard is not
 * locked, or what a system gesture does anywhere. The page is not asked; it is only told.
 */
export function theBrowserLeavesFullScreenOnItsOwn(): void {
  cy.window().then((win) => {
    setFullscreenElement(win, null);
  });
}
