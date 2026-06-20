# Fluent Test Guidelines

Language-agnostic principles and patterns. Apply these regardless of language or framework. Examples use pseudo-code for clarity — see the language-specific references for idiomatic implementations.

## The Three-Act Structure

Every test is a tiny story: set the scene, do something, check the result.

```
test "cancelling an order refunds the customer" {
  // Given
  order = anOrder().withStatus(PAID).withTotal(49.99)
  customer = aCustomer().withBalance(0)

  // When
  orderService.cancel(order, customer)

  // Then
  expect(customer.balance).toEqual(49.99)
  expect(order.status).toEqual(CANCELLED)
}
```

The three blocks should be visually obvious. A reader skimming the file should see the structure without reading the code.

## Anti-Pattern Catalog

### 1. The Mystery Guest

Bad — reader has no idea what `testData.json` contains or why it matters:

```
test "processes order" {
  data = loadFixture("testData.json")
  result = process(data)
  assert(result.success == true)
}
```

Good — the test shows exactly what matters:

```
test "processes a paid order with two line items" {
  // Given
  order = anOrder()
    .withStatus(PAID)
    .withItems(anItem("Widget", 2), anItem("Gadget", 1))

  // When
  result = orderProcessor.process(order)

  // Then
  expect(result).toBeSuccessful()
  expect(result.itemCount).toEqual(3)
}
```

### 2. The Giant Constructor

Bad — most fields are irrelevant noise:

```
test "user full name" {
  user = new User("John", "Doe", 30, "john@test.com", true, null, [], "admin", "2024-01-01")
  expect(user.fullName()).toEqual("John Doe")
}
```

Good — show only what matters:

```
test "full name is derived from first and last name" {
  // Given
  user = aUser().withFirstName("John").withLastName("Doe")

  // When
  name = user.fullName()

  // Then
  expect(name).toEqual("John Doe")
}
```

### 3. The Assertion Roulette

Bad — if this fails, which assertion broke? What was the intent?

```
test "create user" {
  result = createUser(input)
  assert(result != null)
  assert(result.id > 0)
  assert(result.name == "Alice")
  assert(result.email == "alice@example.com")
  assert(result.active == true)
  assert(result.createdAt != null)
}
```

Good — chain domain matchers into a single fluent assertion:

```
test "creates an active user with the provided identity" {
  // Given
  input = aCreateUserRequest()
    .withName("Alice")
    .withEmail("alice@example.com")

  // When
  user = userService.create(input)

  // Then
  expect(user)
    .toHaveValidId()
    .toBeActive()
    .toHaveIdentity(name: "Alice", email: "alice@example.com")
}
```

Each matcher returns the assertion context, so the chain reads as a single sentence: "expect user to have a valid ID, to be active, and to have identity Alice/alice@example.com." If any matcher fails, the failure message pinpoints which one.

### 4. The Logic-Laden Test

Bad — conditionals and loops hide what's actually being tested:

```
test "filter users" {
  users = getUsers()
  active = filter(users, u => u.active)
  for u in active {
    assert(u.active == true)
    if u.role == "admin" {
      assert(u.permissions.length > 0)
    }
  }
}
```

Good — flat, deterministic, one scenario:

```
test "filters only active users from a mixed list" {
  // Given
  users = [
    aUser().withName("Alice").active(),
    aUser().withName("Bob").inactive(),
    aUser().withName("Carol").active(),
  ]

  // When
  result = filterActive(users)

  // Then
  expect(result).toContainExactly("Alice", "Carol")
}
```

### 5. The Vague Name

Bad:
```
test "test service" { ... }
test "it works" { ... }
test "edge case" { ... }
test "bug fix #1234" { ... }
```

Good — names describe behavior, forming a spec when read together:
```
test "returns empty list when no orders match the date range" { ... }
test "applies discount before calculating tax" { ... }
test "rejects payment when card is expired" { ... }
test "retries failed webhook delivery up to three times" { ... }
```

### 6. The Over-Mock

Bad — mocking everything means you're testing the mocks:

```
test "process order" {
  mockRepo = mock(OrderRepo)
  mockPayment = mock(PaymentService)
  mockEmail = mock(EmailService)
  mockLogger = mock(Logger)
  mockMetrics = mock(Metrics)

  when(mockRepo.find(any)).thenReturn(anOrder())
  when(mockPayment.charge(any)).thenReturn(success())
  when(mockEmail.send(any)).thenReturn(ok())

  service = new OrderService(mockRepo, mockPayment, mockEmail, mockLogger, mockMetrics)
  service.process("order-1")

  verify(mockPayment).charge(any)
  verify(mockEmail).send(any)
}
```

Good — mock boundaries, use real objects for internals:

```
test "charges the customer and sends a confirmation email" {
  // Given
  order = anOrder().withTotal(99.50).withCustomerEmail("alice@example.com")
  payments = FakePaymentGateway()
  emails = FakeEmailSender()
  service = new OrderService(payments, emails)

  // When
  service.process(order)

  // Then
  expect(payments.lastCharge()).toHaveAmount(99.50)
  expect(emails.lastSentTo()).toEqual("alice@example.com")
}
```

## Deterministic Assertions

A test must have exactly one code path and assert on exact values. If a test can pass for more than one reason, it's not testing anything.

### Prefer Exact Equality

Loose matchers like `toBeAbove`, `toBeGreaterThan`, `toContain` weaken the assertion — the test passes even when the value drifts far from what's expected.

Bad — passes whether the count is 3 or 3,000,000:

```
test "returns active users" {
  result = userService.findActive()
  expect(result.length).toBeAbove(0)
}
```

Good — assert the exact expectation:

```
test "returns the three active users from seeded data" {
  // Given
  seedUsers([
    aUser().withName("Alice").active(),
    aUser().withName("Bob").active(),
    aUser().withName("Carol").inactive(),
    aUser().withName("Dave").active(),
  ])

  // When
  result = userService.findActive()

  // Then
  expect(result).toContainExactly("Alice", "Bob", "Dave")
}
```

When a loose matcher is genuinely needed (random IDs, timestamps, floating-point), add a code comment explaining why exact equality isn't possible:

```
// UUID is generated at runtime — can only validate format
expect(user.id).toMatchPattern(UUID_REGEX)
```

### No Branching in Tests

A test with `try/catch`, `if/else`, or any conditional makes it possible for the test to pass via different code paths — some of which may not test what you think.

Bad — this test passes whether the operation succeeds OR fails:

```
test "process payment" {
  try {
    result = paymentService.charge(anOrder())
    expect(result.status).toEqual(SUCCESS)
  } catch (error) {
    expect(error.message).toContain("payment failed")
  }
}
```

This is dangerous: if `charge()` throws unexpectedly, the catch block asserts on the error and the test still passes. You've accidentally turned a failure into a passing test.

Good — separate tests, one branch each:

```
test "charges successfully for a valid order" {
  // Given
  order = anOrder().withTotal(49.99)

  // When
  result = paymentService.charge(order)

  // Then
  expect(result.status).toEqual(SUCCESS)
}

test "rejects payment when the card is expired" {
  // Given
  order = anOrder().withCard(anExpiredCard())

  // When / Then
  expect(() => paymentService.charge(order))
    .toThrow("payment failed")
}
```

The rule: **one test = one code path = one outcome**. If your test has branching, it's two tests pretending to be one.

### Summary

| Pattern | Verdict |
|---|---|
| `toEqual(expected)` | Default — always prefer |
| `toBeAbove(0)`, `toBeGreaterThan(n)` | Rare exception — justify with a comment |
| `try/catch` in a test | Never — split into separate tests |
| `if/else` in a test | Never — each branch is a separate test |
| Ternary in an assertion | Never — the expected value must be a literal |

## Mocks vs In-Memory Fakes

Mocks record interactions. Fakes implement behavior. The choice matters as the system grows.

### When Mocks Fall Apart

Mocks work fine with one or two collaborators and simple request-response flows. They start to break when:

- **Many collaborators** — each mock needs setup, and the test becomes a wall of `when(...).thenReturn(...)` that obscures the actual behavior being tested
- **Stateful interactions** — "save then find" requires the mock to remember state across calls, leading to fragile multi-step stubbing
- **Complex behavior** — conditional responses, validation logic, or ordering constraints turn mock setup into a reimplementation of the real thing
- **Refactoring sensitivity** — mocks are coupled to method signatures; renaming or reordering calls breaks tests even when behavior is unchanged

### Prefer In-Memory Fakes

An in-memory fake implements the same interface but with a simple, real implementation:

```
class InMemoryOrderRepo implements OrderRepo {
  orders = []

  save(order) { orders.add(order) }
  findById(id) { return orders.find(o => o.id == id) }
  findByCustomer(customerId) { return orders.filter(o => o.customerId == customerId) }
}
```

Tests using fakes read like production code — no stubbing ceremony:

```
test "finds all orders for a customer after saving them" {
  // Given
  repo = InMemoryOrderRepo()
  repo.save(anOrder().withCustomerId("c1").withTotal(10.00))
  repo.save(anOrder().withCustomerId("c1").withTotal(20.00))
  repo.save(anOrder().withCustomerId("c2").withTotal(5.00))

  // When
  orders = repo.findByCustomer("c1")

  // Then
  expect(orders).toHaveSize(2)
  expect(orders.totalSum()).toEqual(30.00)
}
```

Compare the same test with mocks — the entire behavior is duplicated in the setup:

```
test "finds all orders for a customer after saving them" {
  // Given
  mockRepo = mock(OrderRepo)
  when(mockRepo.findByCustomer("c1")).thenReturn([
    anOrder().withCustomerId("c1").withTotal(10.00),
    anOrder().withCustomerId("c1").withTotal(20.00),
  ])

  // When
  orders = mockRepo.findByCustomer("c1")

  // Then — you're testing your own mock setup, not real behavior
  expect(orders).toHaveSize(2)
}
```

### When to Use Each

| Situation | Use |
|---|---|
| Stateful collaborator (repo, cache, store) | **In-memory fake** — state flows naturally |
| External boundary (HTTP API, message queue) | **Fake or mock** — fake if behavior matters, mock if you only care about the call |
| Fire-and-forget side effect (email, analytics, audit log) | **Mock** — verify it was called with the right arguments |
| Expensive resource (filesystem, clock, random) | **Fake** — deterministic control without mocking internals |

The rule of thumb: if you're stubbing more than two methods on the same mock, write a fake instead.

## Builder Pattern

Builders give every test object sensible defaults. Tests override only the fields relevant to the scenario.

```
function aUser() {
  return UserBuilder {
    firstName: "Default",
    lastName: "User",
    email: "default@example.com",
    active: true,
    role: "member",
  }
}

// In tests — show only what matters:
aUser().withRole("admin")
aUser().withEmail("alice@example.com").inactive()
aUser().withFirstName("Bob").withLastName("Smith")
```

Rules for builders:
- Default values should be valid — a bare `aUser()` produces a usable object
- Method names read as English: `.withEmail(...)`, `.active()`, `.inactive()`
- Return a new instance (immutable) or `this` (mutable chain) — be consistent within the project
- Name the factory function `a<Thing>()` or `an<Thing>()`

## Test Helper Input Validation

Test helpers (page objects, drivers, setup functions) should fail fast when given incomplete or invalid input. Silently ignoring missing values hides bugs.

Bad — silently skips fields the test forgot to provide:

```
function fillForm(data) {
  if (data.name) fillField("Name", data.name)
  if (data.email) fillField("Email", data.email)
  if (data.phone) fillField("Phone", data.phone)
}

test "submits the contact form" {
  fillForm({ name: "Alice" })  // email and phone silently skipped — is that intentional?
  submitForm()
  expect(confirmation()).toBeVisible()  // passes, but the form was incomplete
}
```

Good — require complete input, make partial input explicit:

```
function fillForm(data: ContactForm) {
  fillField("Name", data.name)
  fillField("Email", data.email)
  fillField("Phone", data.phone)
}

function fillFormPartially(data: Partial<ContactForm>) {
  if (data.name) fillField("Name", data.name)
  if (data.email) fillField("Email", data.email)
  if (data.phone) fillField("Phone", data.phone)
}
```

Now the test makes its intent clear:

```
test "submits a complete contact form" {
  fillForm({ name: "Alice", email: "alice@example.com", phone: "555-1234" })
  // ...
}

test "shows validation error when email is missing" {
  fillFormPartially({ name: "Alice" })  // explicit: we're testing the incomplete case
  submitForm()
  expect(validationError()).toContain("Email is required")
}
```

Guidelines:
- Use the type system to enforce required fields — prefer full types over `Partial<T>` for the happy path
- When `Partial` is needed, name the helper differently (`fillFormPartially`, `withOptionalAddress`) so the test communicates intent
- Validate that unknown keys aren't silently ignored — a typo in a field name shouldn't produce a passing test with missing data

## Custom Matchers

Matchers replace low-level assertions with domain language.

Instead of:
```
assert(response.status == 200)
assert(response.body != null)
assert(response.body.items.length > 0)
```

Write a matcher:
```
expect(response).toBeSuccessfulWithItems()
```

Instead of:
```
assert(user.active == true)
assert(user.emailVerified == true)
assert(user.lastLoginAt != null)
```

Write a matcher:
```
expect(user).toBeFullyOnboarded()
```

Guidelines:
- Matcher names start with `toBe`, `toHave`, `toContain`, `toMatch`
- Matchers produce clear failure messages: "expected user to be fully onboarded but email was not verified"
- Compose matchers from simpler ones rather than building monoliths
- Keep matchers in a shared test-utils file, colocated with the builders

## Testing Async Code

Async behavior is where tests become flaky. The root cause is almost always the same: the test has no reliable way to know when the async work is done, so it guesses with delays or polls blindly.

### Anti-Pattern: Sleep and Pray

Bad — arbitrary delays make tests slow and flaky:

```
test "sends a welcome email after registration" {
  // When
  userService.register(aUser())
  sleep(2000)  // hope the async handler finished

  // Then
  expect(emailService.lastSent()).toBeWelcomeEmail()
}
```

If the async handler takes 100ms, you waste 1.9 seconds. If it takes 2.1 seconds under load, the test fails. There is no correct sleep duration.

### Pattern: Event-Based Synchronization

The best async tests don't poll — they subscribe to a signal that the work is done.

```
test "sends a welcome email after registration" {
  // Given
  emailSent = eventBus.waitFor("email.sent")

  // When
  userService.register(aUser())

  // Then
  event = await emailSent
  expect(event.recipient).toEqual("alice@example.com")
  expect(event.template).toEqual("welcome")
}
```

The test waits for exactly the event it cares about. No guessing, no wasted time, no flakiness.

### Pattern: Eventually

When there's no event to subscribe to, use an `eventually` helper that polls a condition with a timeout:

```
test "updates the read model after processing the command" {
  // When
  commandBus.send(aCreateOrderCommand().withId("order-1"))

  // Then
  eventually(timeout: 5000, interval: 100) {
    order = readModel.findById("order-1")
    expect(order).toExist()
    expect(order.status).toEqual(CREATED)
  }
}
```

`eventually` retries the block until it passes or the timeout expires. This is strictly better than a fixed sleep because:
- It passes as soon as the condition is met (fast on happy path)
- The timeout is a safety net, not an expected duration
- The failure message shows _what_ condition wasn't met, not just "timed out"

### Pattern: Return the Promise

The simplest fix is often to make the production code return a signal:

Bad — fire-and-forget with no way to synchronize:

```
class OrderService {
  submit(order) {
    queue.enqueue(order)  // returns void, test has no handle
  }
}
```

Good — return a handle the caller (and tests) can await:

```
class OrderService {
  submit(order) -> Promise<OrderResult> {
    return queue.enqueue(order)  // now tests can await completion
  }
}
```

```
test "submits an order and returns the confirmed result" {
  // When
  result = await orderService.submit(anOrder())

  // Then
  expect(result).toBeConfirmed()
}
```

No polling, no events, no flakiness — because the production code was designed to be testable.

### Improving Production Code Testability

When you encounter async code that's hard to test, consider whether the production code should change. Tests struggling with timing are a design signal, not just a test problem.

| Symptom in tests | Production code improvement |
|---|---|
| Sleep/delay before assertions | Return a promise/future from the async operation |
| Polling a database for a result | Emit a domain event when the state changes |
| Mocking a timer or scheduler | Accept a clock/scheduler as a dependency |
| Retrying because "sometimes it's not ready" | Provide a completion callback or status signal |
| Testing a background job by checking side effects | Split the job into a trigger (enqueue) and a handler (pure logic) — test the handler synchronously |

The goal: every async operation should give the caller a way to know when it's done. If your test needs `eventually`, ask whether the production code _could_ provide a direct signal instead. Reserve `eventually` for cases where you genuinely can't control the producer (third-party systems, legacy code, distributed caches).

## Test Timeouts

Tests should use the shortest timeout that reliably passes. A generous timeout hides performance regressions — a test that used to take 50ms but now takes 3 seconds still passes with a 10-second timeout, and nobody notices until the suite takes an hour.

### Timeout Budgets

| Test type | Target per test | Hard ceiling |
|---|---|---|
| Unit test | < 100ms | 100ms |
| Integration test | < 500ms | 1,000ms |
| E2E (Playwright, Cypress) | < 5s | 10s |

These are per-test budgets, not per-suite. A unit test suite with 200 tests should finish in seconds, not minutes.

### Exceeding the Budget

Any timeout that exceeds the ceiling for its test type must have a code comment explaining why:

```
// 30s timeout: this test loads a 50-page PDF and waits for all pages
// to render sequentially — there's no way to parallelize the rendering
test("renders all pages of a large PDF", async () => { ... }, 30_000)
```

```
// 15s timeout: Stripe webhook delivery has a documented p99 of ~12s
// in sandbox environments
eventually(timeout: 15_000) { ... }
```

Without a justification, the next person to see a large timeout will either:
- Assume it's arbitrary and lower it (breaking the test)
- Assume it's necessary and copy it everywhere (hiding regressions)

Both outcomes are bad. The comment prevents both.

### Slow Tests Are a Design Signal

When a test needs a long timeout, ask why before accepting it:

| Symptom | Question to ask |
|---|---|
| Unit test > 100ms | Is it doing I/O? That's an integration test, move it |
| Integration test > 1s | Can the dependency be replaced with an in-memory fake? |
| E2E test > 10s | Is it testing too many things in one flow? Split it |
| Any test with `sleep()` | Can the production code return a promise or emit an event instead? |

A slow test is often a test that's at the wrong level of the testing pyramid, or production code that isn't designed for testability.

## Naming Convention

Test names should:
- Describe the **behavior**, not the method: "rejects expired cards" not "test validateCard"
- Read as a **sentence** without the `test` keyword
- State the **outcome**: "returns empty list when..." not "handles empty input"
- Include the **condition** when relevant: "applies discount when cart total exceeds 100"
- Avoid `should` prefix — it adds words without meaning
