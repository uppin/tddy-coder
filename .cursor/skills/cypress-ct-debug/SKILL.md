---
name: cypress-ct-debug
description: Debug Cypress component tests by capturing browser console logs with timestamps, analyzing test timelines, and isolating failing tests. Use when Cypress component tests are failing, when investigating timing issues, race conditions, or when the user asks to debug component tests.
---

# Cypress Component Test Debugging

Effective debugging strategies for Cypress component tests, focusing on timeline analysis and browser console capture.

## When to Use This Skill

- Cypress component tests are failing without clear error messages
- Investigating race conditions or timing issues
- Need to see execution order of async operations
- Want to understand why events aren't being handled
- Debugging React state updates and useEffect timing

## Quick Start: Running Tests in Isolation

### Run all component tests

From repo root (uses nix dev shell):
```bash
./dev bun run cypress:component
```

Or from `packages/tddy-web`:
```bash
bun run cypress:component
```

### Run a single test file

```bash
cd packages/tddy-web
ELECTRON_EXTRA_LAUNCH_ARGS='--disable-gpu --no-sandbox' cypress run --component --spec "cypress/component/path/to/YourTest.cy.tsx"
```

### Run a single test case within a file

**Method 1: Using .only() (recommended, most reliable)**
```typescript
// In your test file
it.only('should keep both placeholders loading', () => {
  // ... test code
});
```

Then run the entire file — only the `.only()` test will execute:
```bash
cd packages/tddy-web
bun run cypress:component
```

**Important**: Remove `.only()` before committing! It will cause other tests to be skipped in CI.

**Note**: Cypress CLI does not have a native `--grep` flag for filtering individual test cases. The `--spec` flag only filters by file path, not by test name. Use `.only()` for command-line isolation of specific test cases.

### Why run single tests?
- **Faster iteration**: Often 5-10x faster
- Clearer output without noise from other tests
- Easier to correlate logs with specific test behavior
- Focused debugging on one scenario at a time

**Pro tip**: Add `.only()` immediately when you identify a failing test. Don't waste time running all tests repeatedly during debugging.

## Capturing Browser Console Logs

Browser console.log statements don't appear in Cypress output by default. Here's how to capture them.

### Step 1: Set up log capture (in cypress/support/component.ts)

The existing support file at `packages/tddy-web/cypress/support/component.ts` mounts React components. Add log capture there:

```typescript
import { mount } from "cypress/react";

Cypress.Commands.add("mount", mount);

beforeEach(() => {
  cy.window({log: false}).then((win) => {
    (win as any).cypressLogs = [];
    const originalLog = win.console.log;
    (win.console as any).log = function (...args: any[]) {
      const message = args.map(a =>
        (typeof a === 'object' ? JSON.stringify(a) : String(a))
      ).join(' ');

      if (message.includes('[YourTag]')) {
        (win as any).cypressLogs.push(message);
      }

      originalLog.apply(win.console, args);
    };
  });
});

afterEach(() => {
  cy.window({log: false}).then((win) => {
    const logs = (win as any).cypressLogs || [];
    logs.forEach((log: string) => {
      cy.task('log', `[BROWSER] ${log}`, { log: false });
    });
  });
});
```

### Step 2: Set up file logging (in cypress.config.ts)

Add the `setupNodeEvents` handler to the existing config at `packages/tddy-web/cypress.config.ts`:

```typescript
import { defineConfig } from "cypress";

export default defineConfig({
  component: {
    devServer: {
      framework: "react",
      bundler: "vite",
    },
    specPattern: "cypress/component/**/*.cy.{ts,tsx}",
    supportFile: "cypress/support/component.ts",
    setupNodeEvents(on, _config) {
      const fs = require('fs');
      const path = require('path');
      const logFile = path.join(__dirname, 'cypress-debug.log');

      fs.writeFileSync(logFile, '');

      on('task', {
        log(message) {
          console.log(message);
          fs.appendFileSync(logFile, message + '\n');
          return null;
        },
      });
    },
  },
  e2e: {
    baseUrl: process.env.CYPRESS_BASE_URL ?? "http://localhost:6006",
    specPattern: "cypress/e2e/**/*.cy.{ts,tsx}",
    supportFile: "cypress/support/e2e.ts",
  },
  video: false,
  screenshotOnRunFailure: false,
});
```

### Step 3: Add timestamps to your code

In your React code being tested:

```typescript
console.log(`[${new Date().toISOString()}] [ComponentName] Event happened`, { data });
```

## Timeline Analysis Strategy

Once logs are captured, analyze the execution timeline:

### Extract the timeline

```bash
cd packages/tddy-web

# Get timeline for specific test
grep "test-name" cypress-debug.log

# Get specific event sequence
grep -E "\[TEST\]|\[handleEvent\]|\[useEffect\]" cypress-debug.log | grep "item-id"
```

### Analyze execution order

Look for:

1. **Event dispatch vs handler execution**
   ```
   [12:34:56.100Z] [TEST] triggerEvent - payload
   [12:34:56.105Z] [handleEvent] START - processing  ✅ Handler called
   ```

2. **Missing handlers** (events without responses)
   ```
   [12:34:56.100Z] [TEST] triggerEvent - payload
   [12:34:56.200Z] [TEST] anotherEvent - payload     ❌ No handler for first event!
   ```

3. **useEffect cleanup timing**
   ```
   [12:34:56.100Z] [useEffect] Setting up listener
   [12:34:56.150Z] [useEffect] Cleanup - removing listener
   [12:34:56.160Z] [TEST] triggerEvent                ❌ Event after cleanup!
   ```

4. **State update sequences**
   ```
   [12:34:56.100Z] [setState] Updating status: loading
   [12:34:56.105Z] [useEffect] Check: {status: 'loading'}
   [12:34:56.200Z] [setState] Updating status: success
   [12:34:56.205Z] [useEffect] Check: {status: 'success'}
   ```

### Common Timeline Issues

| Pattern | Problem | Fix |
|---------|---------|-----|
| Event dispatched, no handler logs | Listener not attached or detached | Check useEffect cleanup timing |
| Handler called 10x for single event | Multiple listeners registered | Ensure useEffect cleanup removes listeners, or use module-level deduplication |
| State updates don't trigger effects | Stale closure or wrong dependencies | Use functional setState, check deps array |
| Operations complete but UI doesn't update | React key prop causes incorrect reconciliation | Use stable unique IDs for keys |
| Logs show function runs AFTER it should | Function not running synchronously | React may defer state updater functions — read state directly |

### Counting Occurrences to Detect Issues

```bash
cd packages/tddy-web

# Expected 1, got 10? → Duplicate listeners!
grep "handleEvent.*received" cypress-debug.log | wc -l

# Expected 2, got 20? → Serious duplication
grep "\[addItem\] START" cypress-debug.log | wc -l
```

**Pattern**: When behavior is odd, count occurrences. If count != expected, you've found the problem area.

## Debugging React Hooks in Cypress

### Issue: Multiple duplicate logs (React Strict Mode)

**Cause**: React Strict Mode (React 19 uses this in development) runs effects multiple times in the same millisecond.

**Solution for side effects** (like event listeners): Use module-level deduplication:

```typescript
const eventListenerLocks = new Set<string>();

useLayoutEffect(() => {
  if (eventListenerLocks.has(itemId)) {
    return;
  }

  eventListenerLocks.add(itemId);
  window.addEventListener(eventName, handler);

  return () => {
    window.removeEventListener(eventName, handler);
    eventListenerLocks.delete(itemId);
  };
}, [itemId]);
```

### Issue: useEffect cleanup happens between events

**Cause**: Component re-render or remount between events.

**Solutions**:
1. Check if your useEffect dependencies are stable
2. Use `useRef` for values that shouldn't trigger re-renders
3. Consider using a ref-based event system instead of window events

### Issue: State updates don't persist

**Cause**: Non-functional setState with stale closure.

**Solution**: Use functional updates:
```typescript
// Bad — uses stale closure
setState({...state, status: 'success'});

// Good — uses current state
setState(prev => ({...prev, status: 'success'}));
```

### Issue: State updater function doesn't run immediately

```typescript
setItems(prev => {
  claimedId = prev.find(...).id;  // Sets outer variable
  return prev;
});
console.log('AFTER call', claimedId);  // ❌ undefined!
```

**Cause**: When `setItems` is a wrapper function, React may defer execution.

**Solution**: Read state directly from the state variable:
```typescript
const available = items.find(...);  // Direct read, always synchronous
const claimedId = available?.id;
```

**Detect**: Use Before/After logging:
```typescript
console.log('BEFORE setItems');
setItems(prev => {
  console.log('INSIDE setItems');
  return prev;
});
console.log('AFTER setItems');
```

If logs show: BEFORE -> AFTER -> INSIDE, the function is deferred.

## Avoid Cypress Commands in Synchronous Callbacks

**Critical**: Never call Cypress commands (`cy.log()`, `cy.task()`) from within synchronous callbacks like `console.log` overrides.

```typescript
// ❌ BAD — cy commands in sync context
win.console.log = (...args) => {
  cy.task('log', message);  // Causes "returning promise and commands" error
  originalLog(...args);
};

// ✅ GOOD — Collect synchronously, output in afterEach
win.console.log = (...args) => {
  (win as any).logs.push(message);  // Collect synchronously
  originalLog(...args);
};
```

**Pattern**: Always **collect** data synchronously, then **output** with Cypress commands asynchronously (in `cy.then()`, `afterEach()`, etc.).

## Debugging Workflow

1. **Run the failing test in isolation** — add `.only()` and run via `bun run cypress:component`

2. **Check if test passes/fails consistently** — flaky = timing/race condition; consistent = logic error

3. **Add timestamp logs** to event handlers, state updates, useEffect blocks, async operations

4. **Run test and capture timeline**
   ```bash
   cd packages/tddy-web
   bun run cypress:component
   cat cypress-debug.log | grep "my-test-id" > timeline.txt
   ```

5. **Analyze timeline** for events without handlers, wrong ordering, cleanup between events, lost state updates

6. **Fix the root cause** — stabilize useEffect deps, use functional setState, fix React keys, add event queuing

7. **Remove debug logs** before committing

## Quick Reference Commands

```bash
# Run all component tests (from repo root)
./dev bun run cypress:component

# Run all component tests (from packages/tddy-web)
bun run cypress:component

# Run with debug output
bun run cypress:component:debug

# Run single test case: add .only() to the test, then run file
it.only('test name', () => { ... })

# Run specific test file (from packages/tddy-web)
ELECTRON_EXTRA_LAUNCH_ARGS='--disable-gpu --no-sandbox' cypress run --component --spec "cypress/component/MyTest.cy.tsx"

# View timeline for specific test
cat cypress-debug.log | grep "test-identifier" | less

# Extract event sequence
grep -E "\[handle|\[TEST\]|\[useEffect\]" cypress-debug.log

# Count duplicate events (detect duplication issues)
grep "specific-event" cypress-debug.log | wc -l

# Find logs between timestamps
awk '/T05:26:50/,/T05:26:52/' cypress-debug.log
```

## Common Pitfalls

1. **Not using `.only()` immediately**: Running all tests repeatedly wastes time during debugging
2. **Forgetting to remove `.only()` and debug logs**: Use `grep -r "\.only\|console.log.*\[20" .` before committing
3. **Too many logs**: Filter early (in beforeEach) to keep logs manageable
4. **Unclear log tags**: Use consistent, searchable prefixes like `[ComponentName.method]`
5. **Not using timestamps**: Without timestamps, you can't determine event order
6. **Not counting occurrences**: When behavior is odd, count with `wc -l`
7. **Calling Cypress commands in sync callbacks**: Collect data synchronously, output with Cypress commands in `afterEach()` or `cy.then()`
