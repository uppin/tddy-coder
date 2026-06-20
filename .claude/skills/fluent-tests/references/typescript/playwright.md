# Fluent Test Patterns — Playwright

## Extending Expect

Playwright supports custom matchers via `expect.extend`:

```typescript
// test-utils/matchers.ts
import { expect as baseExpect } from '@playwright/test';

export const expect = baseExpect.extend({
  async toHaveItemCount(page: Page, expected: number) {
    const count = await page.getByTestId('item-list').getByRole('listitem').count();
    return {
      pass: count === expected,
      message: () => `expected ${expected} items, found ${count}`,
    };
  },

  async toShowEmptyState(page: Page) {
    const empty = page.getByText('No items found');
    const visible = await empty.isVisible();
    return {
      pass: visible,
      message: () => `expected empty state to be visible`,
    };
  },
});
```

Tests:

```typescript
test('shows empty state when all items are deleted', async ({ page }) => {
  // Given
  await seedItems(page, ['Widget', 'Gadget']);

  // When
  await deleteAllItems(page);

  // Then
  await expect(page).toShowEmptyState();
});

test('displays the correct item count after adding items', async ({ page }) => {
  // Given
  await page.goto('/items');

  // When
  await addItem(page, 'New Widget');

  // Then
  await expect(page).toHaveItemCount(1);
});
```

## Page Object Model

```typescript
class TodoPage {
  constructor(private page: Page) {}

  async goto() {
    await this.page.goto('/todos');
  }

  async addTodo(text: string) {
    await this.page.getByPlaceholder('What needs to be done?').fill(text);
    await this.page.getByPlaceholder('What needs to be done?').press('Enter');
  }

  async completeTodo(text: string) {
    const row = this.page.getByRole('listitem').filter({ hasText: text });
    await row.getByRole('checkbox').check();
  }

  todoItem(text: string) {
    return this.page.getByRole('listitem').filter({ hasText: text });
  }

  async todoCount() {
    return this.page.getByTestId('todo-count').textContent();
  }
}
```

```typescript
test('marks a todo as completed', async ({ page }) => {
  // Given
  const todos = new TodoPage(page);
  await todos.goto();
  await todos.addTodo('Buy groceries');

  // When
  await todos.completeTodo('Buy groceries');

  // Then
  await expect(todos.todoItem('Buy groceries')).toHaveClass(/completed/);
  await expect(await todos.todoCount()).toBe('0 items left');
});
```
