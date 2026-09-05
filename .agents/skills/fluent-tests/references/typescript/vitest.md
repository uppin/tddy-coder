# Fluent Test Patterns — Vitest

## Builders with TypeScript Types

Use `Partial<T>` overrides so the builder stays type-safe without requiring every field:

```typescript
interface User {
  id: string;
  name: string;
  email: string;
  role: 'admin' | 'member';
  active: boolean;
}

function aUser(overrides: Partial<User> = {}): User {
  return {
    id: 'user-1',
    name: 'Alice',
    email: 'alice@example.com',
    role: 'member',
    active: true,
    ...overrides,
  };
}

function anOrder(overrides: Partial<Order> = {}): Order {
  return {
    id: 'order-1',
    userId: 'user-1',
    items: [anOrderItem()],
    status: 'pending',
    total: 29.99,
    ...overrides,
  };
}
```

Usage — override only what matters:

```typescript
it('grants admin access to users with the admin role', () => {
  // Given
  const user = aUser({ role: 'admin' });

  // When
  const access = resolvePermissions(user);

  // Then
  expect(access).toContain('admin-panel');
});
```

## Custom Matchers

Extend `expect` with domain-specific matchers:

```typescript
// test-utils/matchers.ts
expect.extend({
  toBeActive(received: User) {
    return {
      pass: received.active,
      message: () =>
        `expected user "${received.name}" to be active, but active was ${received.active}`,
    };
  },

  toHaveTotal(received: Order, expected: number) {
    const pass = Math.abs(received.total - expected) < 0.01;
    return {
      pass,
      message: () =>
        `expected order total to be ${expected}, but was ${received.total}`,
    };
  },
});
```

Usage reads like English:

```typescript
it('activates a new user after email verification', () => {
  // Given
  const user = aUser({ active: false, emailVerified: false });

  // When
  const updatedUser = verifyEmail(user, 'valid-token');

  // Then
  expect(updatedUser).toBeActive();
});

it('applies a percentage discount to the order total', () => {
  // Given
  const order = anOrder({ items: [anOrderItem({ price: 100 })], discount: 10 });

  // When
  const discounted = applyDiscount(order);

  // Then
  expect(discounted).toHaveTotal(90);
});
```

## Structuring Describe Blocks

One level of nesting. Group by feature, not by method:

```typescript
describe('OrderService', () => {
  it('creates an order with the given line items', () => { ... });

  it('rejects an order when the cart is empty', () => { ... });

  it('applies a percentage discount before tax calculation', () => { ... });

  it('sends a confirmation email after successful creation', () => { ... });
});
```

Avoid deeply nested describes:

```typescript
// Bad — too much nesting
describe('OrderService', () => {
  describe('create', () => {
    describe('when cart is empty', () => {
      describe('and user is guest', () => {
        it('should throw', () => { ... });
      });
    });
  });
});
```

## Async Tests

Keep the three-act structure clear even with `async/await`:

```typescript
it('fetches and transforms the user profile', async () => {
  // Given
  const api = aFakeApi().withUser('user-1', aUser({ name: 'Alice' }));

  // When
  const profile = await profileService.fetch('user-1');

  // Then
  expect(profile.displayName).toBe('Alice');
});
```
