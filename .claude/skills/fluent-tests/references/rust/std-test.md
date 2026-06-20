# Fluent Test Patterns — Rust

Idiomatic patterns for the built-in `#[test]` harness, extended with `rstest` for
parameterized tests and fixtures, `tokio::test` for async, and `pretty_assertions` for
readable diffs. The fluent principles are identical to every other language — Rust just
expresses builders and matchers through `impl` blocks and extension traits instead of
classes.

## Three-Act Structure

Rust test modules live next to the code in `#[cfg(test)] mod tests`. Keep the Given / When /
Then blocks visually separated with comments:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_util::{an_order, an_item};

    #[test]
    fn applies_percentage_discount_before_calculating_the_total() {
        // Given
        let order = an_order()
            .with_items(vec![an_item("Widget", 10.00, 3)])
            .with_discount(Discount::percent(10));

        // When
        let total = OrderService::default().calculate_total(&order);

        // Then
        assert_eq!(total, 27.00);
    }

    #[test]
    fn rejects_an_order_with_no_items() {
        // Given
        let order = an_order().with_no_items();

        // When
        let result = OrderService::default().submit(order);

        // Then
        assert_validation_error(result).has_message_containing("at least one item");
    }
}
```

## Test Names as a Specification

The function name *is* the sentence — no `test_` prefix, no `should`. `cargo test` and
`cargo nextest` print the names, so they read as a spec:

```rust
#[test]
fn creates_an_active_user_with_a_generated_uuid() { /* ... */ }

#[test]
fn rejects_registration_when_the_email_is_already_taken() { /* ... */ }

#[test]
fn deactivates_a_user_and_revokes_all_active_sessions() { /* ... */ }

#[test]
fn returns_none_when_the_user_id_does_not_exist() { /* ... */ }
```

```
running 4 tests
test tests::creates_an_active_user_with_a_generated_uuid ... ok
test tests::rejects_registration_when_the_email_is_already_taken ... ok
test tests::deactivates_a_user_and_revokes_all_active_sessions ... ok
test tests::returns_none_when_the_user_id_does_not_exist ... ok
```

Avoid `test_create_user` (names the method), `it_works` (vague), `handles_empty_input`
(says what it handles, not what it does).

## Builder Pattern

Rust builders use owned `self` and return `Self` so calls chain. Defaults come from a plain
constructor function named `a_thing()` / `an_thing()` — a bare `an_order()` must produce a
valid, usable value.

```rust
// test_util/builders.rs — compiled only for tests
pub struct OrderBuilder {
    id: String,
    customer_id: String,
    items: Vec<OrderItem>,
    status: OrderStatus,
    discount: Option<Discount>,
}

pub fn an_order() -> OrderBuilder {
    OrderBuilder {
        id: "order-1".into(),
        customer_id: "customer-1".into(),
        items: vec![an_item("Widget", 10.00, 1)],
        status: OrderStatus::Pending,
        discount: None,
    }
}

impl OrderBuilder {
    pub fn with_id(mut self, id: &str) -> Self {
        self.id = id.into();
        self
    }

    pub fn with_items(mut self, items: Vec<OrderItem>) -> Self {
        self.items = items;
        self
    }

    pub fn with_no_items(mut self) -> Self {
        self.items = vec![];
        self
    }

    pub fn with_status(mut self, status: OrderStatus) -> Self {
        self.status = status;
        self
    }

    pub fn with_discount(mut self, discount: Discount) -> Self {
        self.discount = Some(discount);
        self
    }

    pub fn build(self) -> Order {
        Order {
            id: self.id,
            customer_id: self.customer_id,
            items: self.items,
            status: self.status,
            discount: self.discount,
        }
    }
}

pub fn an_item(name: &str, price: f64, quantity: u32) -> OrderItem {
    OrderItem { name: name.into(), price, quantity }
}
```

A builder that mirrors a method on the type under test can pass `&self` straight through, but
if `Order` derives `Default` you can skip the builder for trivial cases:

```rust
let order = Order { status: OrderStatus::Paid, ..Default::default() };
```

Prefer the builder once a test needs to express more than one or two meaningful fields — it
keeps the *what matters* visible and the noise hidden.

## Custom Assertions via Extension Traits

Rust's equivalent of a custom matcher is an extension trait that adds `assert_*` methods to a
domain type. Each method returns `&Self` so assertions chain into one fluent sentence, and
each carries a message that pinpoints the failure.

```rust
// test_util/assertions.rs
pub trait UserAssertions {
    fn assert_active(&self) -> &Self;
    fn assert_email(&self, expected: &str) -> &Self;
    fn assert_role(&self, expected: Role) -> &Self;
    fn assert_valid_id(&self) -> &Self;
}

impl UserAssertions for User {
    fn assert_active(&self) -> &Self {
        assert!(self.active, "expected user '{}' to be active", self.name);
        self
    }

    fn assert_email(&self, expected: &str) -> &Self {
        assert_eq!(self.email, expected, "user email mismatch");
        self
    }

    fn assert_role(&self, expected: Role) -> &Self {
        assert_eq!(self.role, expected, "user role mismatch");
        self
    }

    fn assert_valid_id(&self) -> &Self {
        assert!(!self.id.is_empty(), "expected a non-empty id");
        self
    }
}
```

Tests chain domain assertions instead of poking at fields:

```rust
#[test]
fn creates_an_active_admin_user_with_the_given_email() {
    // Given
    let request = a_create_user_request()
        .with_email("alice@example.com")
        .with_role(Role::Admin);

    // When
    let user = UserService::default().create(request);

    // Then
    user.assert_active()
        .assert_email("alice@example.com")
        .assert_role(Role::Admin);
}
```

Compose bigger assertions from smaller ones rather than building monoliths:

```rust
impl UserAssertions for User {
    fn assert_fully_onboarded(&self) -> &Self {
        self.assert_active();
        assert!(self.email_verified, "expected '{}' to have a verified email", self.name);
        assert!(self.last_login_at.is_some(), "expected '{}' to have logged in", self.name);
        self
    }
}
```

For richer failure diffs on large structs, pull in `pretty_assertions` in dev-dependencies and
`use pretty_assertions::assert_eq;` — the macro is a drop-in replacement that prints a colored
line diff.

## Deterministic Assertions

Assert exact values, not loose bounds. `assert_eq!` is the default; reserve `>` / `<` checks
for genuinely non-deterministic values (timestamps, random ids) and add a comment.

```rust
// Bad — passes whether the count is 3 or 3,000,000
assert!(service.find_active().len() > 0);

// Good — assert the exact set
let active = service.find_active();
active.assert_contains_exactly(&["Alice", "Bob", "Dave"]);
```

```rust
// UUID is generated at runtime — can only validate the format
assert!(UUID_REGEX.is_match(&user.id), "id was not a uuid: {}", user.id);
```

No `if`/`match`/loops that change which branch asserts — one test is one code path. If you
catch yourself branching on the result, split it into two tests.

## Collection Assertions

Build an extension trait for the assertions you repeat across collections:

```rust
pub trait UserSliceAssertions {
    fn assert_contains_exactly(&self, names: &[&str]) -> &Self;
    fn assert_all_active(&self) -> &Self;
}

impl UserSliceAssertions for Vec<User> {
    fn assert_contains_exactly(&self, names: &[&str]) -> &Self {
        let actual: Vec<&str> = self.iter().map(|u| u.name.as_str()).collect();
        assert_eq!(actual, names, "user set mismatch");
        self
    }

    fn assert_all_active(&self) -> &Self {
        for user in self {
            user.assert_active();
        }
        self
    }
}
```

```rust
#[test]
fn returns_the_three_active_users_from_seeded_data() {
    // Given
    let repo = InMemoryUserRepo::default();
    repo.save(a_user().with_name("Alice").active());
    repo.save(a_user().with_name("Bob").active());
    repo.save(a_user().with_name("Carol").inactive());
    repo.save(a_user().with_name("Dave").active());

    // When
    let active = UserService::new(&repo).find_active();

    // Then
    active.assert_contains_exactly(&["Alice", "Bob", "Dave"]);
}
```

## Error and Panic Assertions

Most fallible Rust code returns `Result`. Assert on the error variant with a domain helper so
the test reads as a sentence — don't bury it in `matches!`:

```rust
pub struct ValidationErrorAssert(OrderError);

pub fn assert_validation_error<T>(result: Result<T, OrderError>) -> ValidationErrorAssert {
    match result {
        Err(e @ OrderError::Validation { .. }) => ValidationErrorAssert(e),
        Err(other) => panic!("expected a validation error but got {other:?}"),
        Ok(_) => panic!("expected a validation error but the call succeeded"),
    }
}

impl ValidationErrorAssert {
    pub fn has_message_containing(self, fragment: &str) -> Self {
        let msg = self.0.to_string();
        assert!(msg.contains(fragment), "expected message to contain '{fragment}', was '{msg}'");
        self
    }
}
```

```rust
#[test]
fn rejects_payment_when_the_card_is_expired() {
    // Given
    let order = an_order().with_card(an_expired_card()).build();

    // When
    let result = PaymentService::default().charge(&order);

    // Then
    assert_validation_error(result).has_message_containing("card is expired");
}
```

A test that itself returns `Result` lets you use `?` for the *Given* setup while still asserting
the outcome explicitly — never let `?` swallow the assertion you mean to make:

```rust
#[test]
fn loads_the_saved_order_by_id() -> anyhow::Result<()> {
    // Given
    let repo = SqliteOrderRepo::in_memory()?;
    repo.save(an_order().with_id("order-1").build())?;

    // When
    let found = repo.find_by_id("order-1")?;

    // Then
    assert_eq!(found.id, "order-1");
    Ok(())
}
```

Reserve `#[should_panic(expected = "...")]` for code whose contract really is to panic
(invariant violations, `unwrap` on a guaranteed value). For ordinary domain errors, prefer the
`Result` assertion above — it's more precise and doesn't pass on an unrelated panic.

## In-Memory Fakes over Mocks

Rust has mocking crates (`mockall`), but a hand-written fake that implements the trait is
usually clearer and survives refactors. State flows naturally and the test reads like
production code with no stubbing ceremony:

```rust
#[derive(Default)]
pub struct InMemoryOrderRepo {
    orders: RefCell<Vec<Order>>,
}

impl OrderRepo for InMemoryOrderRepo {
    fn save(&self, order: Order) {
        self.orders.borrow_mut().push(order);
    }

    fn find_by_customer(&self, customer_id: &str) -> Vec<Order> {
        self.orders
            .borrow()
            .iter()
            .filter(|o| o.customer_id == customer_id)
            .cloned()
            .collect()
    }
}
```

```rust
#[test]
fn finds_all_orders_for_a_customer_after_saving_them() {
    // Given
    let repo = InMemoryOrderRepo::default();
    repo.save(an_order().with_customer_id("c1").with_total(10.00).build());
    repo.save(an_order().with_customer_id("c1").with_total(20.00).build());
    repo.save(an_order().with_customer_id("c2").with_total(5.00).build());

    // When
    let orders = repo.find_by_customer("c1");

    // Then
    assert_eq!(orders.len(), 2);
    assert_eq!(orders.iter().map(|o| o.total).sum::<f64>(), 30.00);
}
```

Use a fake for stateful collaborators (repos, caches, clocks). Reserve `mockall` for
fire-and-forget boundaries where you only care that a call happened (email, metrics). If you
find yourself stubbing more than two methods, write a fake.

For deterministic time and randomness, inject a `Clock` / `IdGenerator` trait and supply a
fixed fake in tests rather than reaching for the system clock.

## Parameterized Tests with `rstest`

When the *same* assertion runs over different inputs, `#[rstest]` with `#[case]` keeps each
case named in the output instead of hiding them in a loop:

```rust
use rstest::rstest;

#[rstest]
#[case::no_at_sign("no-at-sign")]
#[case::missing_local("@missing-local")]
#[case::double_at("double@@at.com")]
#[case::trailing_dot("trailing-dot.@example.com")]
fn rejects_invalid_email_formats(#[case] invalid_email: &str) {
    // Given
    let request = a_create_user_request().with_email(invalid_email);

    // When
    let result = UserService::default().create(request);

    // Then
    assert_validation_error(result).has_message_containing("invalid email");
}
```

`#[values(...)]` covers the cartesian product of several arguments. Reserve all of this for the
*same* behavior across data — if different inputs lead to different outcomes, write separate
tests with descriptive names.

## Fixtures with `rstest`

Extract shared setup into a `#[fixture]` instead of a `setUp`-style mutation. The fixture is a
function; tests declare it as a parameter and get a fresh value each run:

```rust
use rstest::{fixture, rstest};

#[fixture]
fn service() -> OrderService {
    let payments = FakePaymentGateway::default();
    let emails = FakeEmailSender::default();
    OrderService::new(payments, emails)
}

#[rstest]
fn charges_the_customer_for_a_paid_order(service: OrderService) {
    // Given
    let order = an_order().with_total(25.00).with_status(OrderStatus::Paid).build();

    // When
    service.process(order);

    // Then
    assert_eq!(service.payments().last_charge(), 25.00);
}
```

## Async Tests with `tokio::test`

`#[tokio::test]` runs an async test on a runtime. Keep the three-act structure and `.await` the
operation — never sleep to "let it finish":

```rust
#[tokio::test]
async fn submits_an_order_and_returns_the_confirmed_result() {
    // Given
    let service = OrderService::default();

    // When
    let result = service.submit(an_order().build()).await;

    // Then
    assert_eq!(result.status, OrderStatus::Confirmed);
}
```

When work is fire-and-forget, prefer waiting on a signal (a `oneshot`/`watch` channel, or a
`Notify`) over `tokio::time::sleep`. If you must poll a condition, use `tokio::time::timeout`
around a small retry loop so the failure says *what* never happened rather than just hanging:

```rust
#[tokio::test]
async fn updates_the_read_model_after_processing_the_command() {
    // Given
    let app = TestApp::start().await;

    // When
    app.send(a_create_order_command().with_id("order-1")).await;

    // Then — bounded wait, not a fixed sleep
    let order = timeout(Duration::from_secs(5), app.await_order("order-1"))
        .await
        .expect("read model never produced order-1");
    assert_eq!(order.status, OrderStatus::Created);
}
```

A test struggling with timing is a design signal: have the async operation return a future or
emit an event the test can await, rather than checking side effects after a delay. See the
"Testing Async Code" section in `generic-guidelines.md`.

## Keeping Tests Fast

Rust has no built-in per-test timeout, so the discipline is structural: a `#[test]` that does
real I/O is an integration test and belongs in `tests/` with an in-memory or containerized
dependency, not a unit test reaching for the network or filesystem. Unit tests in
`#[cfg(test)] mod tests` should finish in well under a second each — a slow unit test almost
always means a missing fake. Run with `cargo nextest run` for parallel execution and clear
per-test timing.
