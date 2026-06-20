# Fluent Test Patterns — Cypress E2E

## Selectors Live in the Driver, Never the Test

A test should never contain a raw selector string. `cy.get("[data-testid='github-login-button']")`
is noise — it tells the reader *how* to find the element, not *what* the element is. Every
selector belongs inside a page object or custom command, exposed as a named method.

Bad — the selector leaks into the test, repeated verbatim, verbose at every call site:

```typescript
it('logs in', () => {
  cy.get("[data-testid='github-login-button']", { timeout: 10_000 }).should('exist').click();
  cy.get("[data-testid='user-login']", { timeout: 15_000 }).should('have.text', 'testuser');
  cy.get("[data-testid='livekit-url']").should('exist');
});
```

Good — the page object owns the selectors; the test names intent:

```typescript
// cypress/support/pages/auth.ts
const byTestId = (id: string, options?: Parameters<typeof cy.get>[1]) =>
  cy.get(`[data-testid='${id}']`, options);

export const authPage = {
  loginButton: () => byTestId('github-login-button', { timeout: 10_000 }),
  userLogin: () => byTestId('user-login', { timeout: 15_000 }),
  connectionForm: () => byTestId('livekit-url'),
};
```

```typescript
it('shows the user identity after a successful login', () => {
  authPage.loginButton().should('exist').click();
  authPage.userLogin().should('have.text', 'testuser');
  authPage.connectionForm().should('exist');
});
```

Rules:
- **No raw `cy.get("[data-testid=…]")`, `cy.get('.class')`, or XPath in a test body** — only named driver/page-object methods.
- Keep a single `byTestId` (or `byHook`) helper in the driver so the verbose attribute syntax appears **once**, not at every call site.
- Prefer semantic queries (`cy.findByRole`, `cy.findByLabelText`) over test ids where the accessible name is stable — but those still live in the driver, not the test.
- A `data-testid` string is a magic value: if it changes, exactly one driver method changes, and every test keeps working.

This is the rule the rest of this file assumes — the page objects and custom commands below exist
precisely so selectors stay out of tests.

## Custom Commands as Fluent Steps

Register custom commands that read as actions:

```typescript
// cypress/support/commands.ts
Cypress.Commands.add('login', (email: string) => {
  cy.visit('/login');
  cy.findByLabelText('Email').type(email);
  cy.findByLabelText('Password').type('test-password');
  cy.findByRole('button', { name: 'Sign In' }).click();
  cy.url().should('include', '/dashboard');
});

Cypress.Commands.add('createOrder', (items: string[]) => {
  cy.visit('/orders/new');
  items.forEach(item => cy.findByText(item).click());
  cy.findByRole('button', { name: 'Place Order' }).click();
});
```

Tests become a readable flow:

```typescript
it('displays the new order in the order list after creation', () => {
  // Given
  cy.login('alice@example.com');

  // When
  cy.createOrder(['Widget', 'Gadget']);

  // Then
  cy.visit('/orders');
  cy.findByText('Widget').should('be.visible');
  cy.findByText('Gadget').should('be.visible');
});
```

## Page Objects for Complex Pages

```typescript
// cypress/support/pages/checkout.ts
interface ShippingAddress {
  street: string;
  city: string;
  zip: string;
}

const SHIPPING_FIELDS: (keyof ShippingAddress)[] = ['street', 'city', 'zip'];

export const checkoutPage = {
  visit: () => cy.visit('/checkout'),
  fillShipping: (address: ShippingAddress) => {
    cy.findByLabelText('Street').type(address.street);
    cy.findByLabelText('City').type(address.city);
    cy.findByLabelText('ZIP').type(address.zip);
  },
  placeOrder: () => cy.findByRole('button', { name: 'Place Order' }).click(),
  confirmationMessage: () => cy.findByTestId('order-confirmation'),
};
```

Note: `fillShipping` requires a full `ShippingAddress`, not `Partial<Address>`. Silently skipping missing fields via `if (address.street)` hides bugs — a test that forgets to provide a zip code would pass without actually filling the form. If a test needs to submit an incomplete form, make that intent explicit with a separate helper like `fillShippingPartially`.

```typescript
it('completes checkout with a valid shipping address', () => {
  // Given
  cy.login('alice@example.com');
  cy.createOrder(['Widget']);

  // When
  checkoutPage.visit();
  checkoutPage.fillShipping({ street: '123 Main St', city: 'Portland', zip: '97201' });
  checkoutPage.placeOrder();

  // Then
  checkoutPage.confirmationMessage().should('contain', 'Order confirmed');
});
```
