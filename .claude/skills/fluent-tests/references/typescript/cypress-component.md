# Fluent Test Patterns — Cypress Component Tests

## Fluent Driver Pattern

Component tests use a driver that wraps mounting, interaction, and assertion into a fluent chain. This keeps the test focused on behavior while the driver handles framework wiring.

Real-world example — a 3D rendering engine tested with a fluent `scenic()` driver:

```typescript
import { expectRender, scenic } from '../support/scenic';

describe('ScenicEngine - Vector Graphics', () => {
  before(() =>
    scenic()
      .boot({
        canvas: 'vector-test-canvas',
        size: [800, 600],
        engine: { headless: false },
      })
      .then(() => scenic().orbitTo([0, 0, 0], { distance: 5 }))
  );

  it('renders a square vector path', () => {
    // Given + When
    const rendered = scenic()
      .orbitTo([0, 0, 0], { distance: 5 })
      .addVector({
        position: [0, 0, 0],
        svgPath: 'M 10 10 L 90 10 L 90 90 L 10 90 Z',
        size: [2, 2],
        color: '#FF0000',
        glowEnabled: true,
        glowIntensity: 1.0,
      })
      .waitFrames(10)
      .captureAs('vector-basic-square');

    // Then
    expectRender(rendered)
      .to.haveNonBlackPixelsAbove(5, { threshold: 10 })
      .and.havePixelsMatching('#C02020', { tolerance: 80, min: 100 });
  });

  it('recovers gracefully after receiving an invalid SVG path', () => {
    // Given — feed an invalid path
    scenic()
      .addVector({
        position: [0, 0, 0],
        svgPath: 'INVALID PATH DATA',
        size: [2, 2],
        color: '#000000',
      });
    scenic().waitFrames(5);

    // When — add a valid shape after the bad one
    const recovery = scenic()
      .addVector({
        position: [0, 0, 0],
        svgPath: 'M 10 10 L 90 10 L 90 90 L 10 90 Z',
        size: [2, 2],
        color: '#00FF00',
        glowEnabled: true,
        glowIntensity: 1.0,
      })
      .waitFrames(10)
      .captureAs('vector-recovery-after-invalid');

    // Then
    expectRender(recovery).to.haveNonBlackPixelsAbove(5, { threshold: 10 });
  });
});
```

Key takeaways from this pattern:
- **`scenic()`** is a fluent driver — chainable methods build up the scene
- **`.captureAs('name')`** returns a renderable reference that flows into assertions
- **`expectRender()`** provides domain-specific matchers (`haveNonBlackPixelsAbove`, `havePixelsMatching`) that chain with `.and`
- Tests read top-to-bottom: configure scene, capture, assert on the capture
- No raw `cy.get()`, no selectors in tests — the driver encapsulates all interaction

## Building Your Own Component Driver

```typescript
// cypress/support/drivers/datepicker-driver.ts
import { mount } from 'cypress/react';

export function aDatePicker(props: Partial<DatePickerProps> = {}) {
  const defaults: DatePickerProps = {
    value: new Date('2024-06-15'),
    locale: 'en-US',
    onChange: cy.stub().as('onChange'),
    ...props,
  };

  return {
    mount: () => {
      mount(<DatePicker {...defaults} />);
      return aDatePicker(props);
    },
    selectDate: (day: number) => {
      cy.findByRole('gridcell', { name: String(day) }).click();
      return aDatePicker(props);
    },
    openCalendar: () => {
      cy.findByRole('button', { name: /open calendar/i }).click();
      return aDatePicker(props);
    },
    expectSelectedDate: (date: string) => {
      cy.findByRole('textbox').should('have.value', date);
    },
    expectCalendarOpen: () => {
      cy.findByRole('dialog').should('be.visible');
    },
  };
}
```

Usage — fluent chain from mount to assertion:

```typescript
it('updates the input when a date is selected from the calendar', () => {
  aDatePicker({ value: new Date('2024-06-01') })
    .mount()
    .openCalendar()
    .selectDate(15)
    .expectSelectedDate('06/15/2024');
});
```

## Waiting for Async Events with Stubs

Components often signal lifecycle events via callbacks (loading started, render complete, data fetched). Use `cy.stub().as('alias')` to turn these callbacks into synchronization points that Cypress retries automatically — no sleeps, no arbitrary waits.

Wrap the stubs inside the driver so the test stays free of framework wiring:

```typescript
// cypress/support/drivers/pdf-stage-driver.ts
import { mount } from 'cypress/react';

export function aPdfStage(props: Partial<PdfStageProps> = {}) {
  const onLoadingStart = cy.stub().as('loadingStart');
  const onLoadingComplete = cy.stub().as('loadingComplete');
  const onRenderComplete = cy.stub().as('renderComplete');

  const defaults: PdfStageProps = {
    asset: aTestAsset({ mediaId: 'essay', pageNumber: 1 }),
    onLoadingStart,
    onLoadingComplete,
    onRenderComplete,
    ...props,
  };

  return {
    mount: () => {
      mount(<PdfStage {...defaults} />);
      return aPdfStage(props);
    },
    expectLoadingStarted: () => {
      cy.get('@loadingStart').should('have.been.called');
      return aPdfStage(props);
    },
    expectLoadingComplete: () => {
      cy.get('@loadingComplete').should('have.been.called');
      return aPdfStage(props);
    },
    expectRenderComplete: (timeout = 10_000) => {
      cy.get('@renderComplete', { timeout }).should('have.been.called');
      return aPdfStage(props);
    },
    expectCanvasVisible: () => {
      cy.get('[data-testid="pdf-canvas"]').should('be.visible');
      return aPdfStage(props);
    },
    expectCanvasDimensions: (minWidth: number, minHeight: number) => {
      cy.get('[data-testid="pdf-canvas"]').then(($canvas) => {
        const canvas = $canvas[0] as HTMLCanvasElement;
        expect(canvas.width).to.be.greaterThan(minWidth);
        expect(canvas.height).to.be.greaterThan(minHeight);
      });
    },
  };
}
```

The test reads as a timeline — mount, wait for lifecycle, assert on result:

```typescript
it('renders a PDF page with correct dimensions after loading completes', () => {
  aPdfStage({ asset: aTestAsset({ mediaId: 'essay', pageNumber: 1 }) })
    .mount()
    .expectLoadingStarted()
    .expectLoadingComplete()
    .expectRenderComplete()
    .expectCanvasVisible()
    .expectCanvasDimensions(595, 842);
});
```

How the stubs work under the hood:
- **`cy.stub().as('name')`** creates a Cypress-tracked stub with an alias
- **`cy.get('@name').should('have.been.called')`** retries until the stub fires — this is Cypress's built-in retry mechanism, not a poll loop
- **Custom timeout** via `{ timeout: ms }` for operations that take longer (rendering, network)

This is the Cypress-native equivalent of event-based synchronization described in the generic guidelines. Prefer this over `cy.wait(ms)` or `cy.clock()` whenever the component exposes lifecycle callbacks.
