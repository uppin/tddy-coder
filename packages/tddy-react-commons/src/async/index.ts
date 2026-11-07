import { Subject, Subscription, type Observable } from 'rxjs';

import { createStore, type Store } from '../stores/index';

import { createPoller, type Poller, type PollOptions } from './poller';

export * from './poller';

/**
 * Where one call to a driven adapter has got to.
 *
 * A discriminated union rather than the `{ data, loading, error }` triple, because
 * that triple can represent states that cannot happen — loading *and* errored,
 * ready with no value — and every consumer then writes its own rules about which
 * combinations to trust.
 *
 * `error` is `unknown`, not `Error`: a rejected promise can carry anything, and
 * narrowing it is the consumer's decision, not a lie this type tells.
 */
export type RequestState<T> =
  | { readonly status: 'idle' }
  | { readonly status: 'loading' }
  | { readonly status: 'ready'; readonly value: T }
  | { readonly status: 'error'; readonly error: unknown };

/** Narrow to the state that carries a value. */
export function isReady<T>(state: RequestState<T>): state is { status: 'ready'; value: T } {
  return state.status === 'ready';
}

/** True while a call is outstanding. */
export function isPending<T>(state: RequestState<T>): boolean {
  return state.status === 'loading';
}

/**
 * One adapter call, with its state projected into a store the view can select on.
 *
 * `invoke` returns nothing. A caller that wanted the promise would be doing its
 * own error handling beside the state this already tracks, which is how a view
 * ends up with two sources of truth about the same call.
 */
export interface Request<Args extends readonly unknown[], T> {
  readonly state: Store<RequestState<T>>;
  invoke(...args: Args): void;
  reset(): void;
  teardown(): void;
}

export interface RequestOptions {
  /**
   * What a second `invoke` does to a call still in flight.
   *
   * `'switch'` (default) abandons the earlier one — right for a search box or a
   * re-send, where only the newest answer matters. `'queue'` runs them in order —
   * right when each call has a side effect that must not be dropped.
   */
  readonly concurrency?: 'switch' | 'queue';
}

/**
 * Wrap a promise-returning adapter method as a `Request`.
 *
 * Rejections become `{ status: 'error' }` and never reach the observable's error
 * channel: an rxjs error terminates the subscription, so one failed call would
 * leave a dead `Request` that silently ignores every later `invoke`.
 */
export function createRequest<Args extends readonly unknown[], T>(
  call: (...args: Args) => Promise<T>,
  options?: RequestOptions,
): Request<Args, T> {
  const concurrency = options?.concurrency ?? 'switch';
  const state = createStore<RequestState<T>>(() => ({ status: 'idle' }));
  let generation = 0;
  let tornDown = false;
  const queuedArgs: Args[] = [];
  let queueBusy = false;

  const writeState = (next: RequestState<T>): void => {
    if (!tornDown) {
      state.next(next);
    }
  };

  const runCall = async (args: Args, gen: number): Promise<void> => {
    writeState({ status: 'loading' });
    try {
      const value = await call(...args);
      if (gen === generation && !tornDown) {
        writeState({ status: 'ready', value });
      }
    } catch (error) {
      if (gen === generation && !tornDown) {
        writeState({ status: 'error', error });
      }
    }
  };

  const runQueuedCall = async (args: Args): Promise<void> => {
    writeState({ status: 'loading' });
    try {
      const value = await call(...args);
      if (!tornDown) {
        writeState({ status: 'ready', value });
      }
    } catch (error) {
      if (!tornDown) {
        writeState({ status: 'error', error });
      }
    }
    queueBusy = false;
    processQueue();
  };

  const processQueue = (): void => {
    if (queueBusy || queuedArgs.length === 0 || tornDown) {
      return;
    }
    queueBusy = true;
    const args = queuedArgs.shift();
    if (args === undefined) {
      queueBusy = false;
      return;
    }
    void runQueuedCall(args);
  };

  const invoke = (...args: Args): void => {
    if (tornDown) {
      return;
    }

    if (concurrency === 'switch') {
      generation += 1;
      const gen = generation;
      void runCall(args, gen);
      return;
    }

    queuedArgs.push(args);
    processQueue();
  };

  return {
    state,
    invoke,
    reset: () => {
      if (tornDown) {
        return;
      }
      generation += 1;
      queuedArgs.length = 0;
      queueBusy = false;
      writeState({ status: 'idle' });
    },
    teardown: () => {
      tornDown = true;
      generation += 1;
      queuedArgs.length = 0;
      queueBusy = false;
    },
  };
}

/**
 * A failure a command could not handle, on its way to whatever renders it.
 *
 * `command` is the name the view asked for, not the adapter method that failed —
 * the view has no idea what a `CreateChatCompletion` is, and the message it shows
 * is about the thing the person tried to do.
 */
export interface CommandError {
  readonly command: string;
  readonly error: unknown;
}

/**
 * The one way a handler reports a failure the view must show.
 *
 * Without this every app invents its own: an error field on a store here, a
 * rejected promise there, a `console.error` in the third place — and a failure
 * that reaches none of them is invisible.
 */
export interface CommandErrorChannel {
  readonly errors$: Observable<CommandError>;
  report(command: string, error: unknown): void;
  /**
   * Run `body`, reporting a rejection rather than leaving it unhandled. Returns
   * whether it succeeded, so a caller can skip follow-up work without catching.
   */
  guard(command: string, body: () => Promise<void>): Promise<boolean>;
  teardown(): void;
}

export function createCommandErrorChannel(): CommandErrorChannel {
  const subject = new Subject<CommandError>();
  let tornDown = false;

  return {
    errors$: subject.asObservable(),
    report: (command, error) => {
      if (!tornDown) {
        subject.next({ command, error });
      }
    },
    guard: async (command, body) => {
      try {
        await body();
        return true;
      } catch (error) {
        if (!tornDown) {
          subject.next({ command, error });
        }
        return false;
      }
    },
    teardown: () => {
      tornDown = true;
      subject.complete();
    },
  };
}

/**
 * Long-lived resources built and disposed as one unit.
 *
 * A handler that constructs its resources at the top of `start` and reverses each
 * of them at the bottom is keeping a checklist nothing checks: add a sixth
 * resource, forget its line, and a poll loop runs for the life of the page while
 * the handler still compiles and the app still works. Pairing the two in one call
 * removes the step there is to forget.
 *
 * A factory per resource kind rather than a `register(resource)` taking anything
 * disposable: registration that is its own call is registration that can be
 * skipped, which is the omission this exists to prevent.
 *
 * Deliberately no generic escape hatch. Everything a handler owns today is one of
 * these four, and a handler that needs to own something else should widen this
 * interface on purpose rather than reach past it with a closure.
 */
export interface AppScope {
  request<Args extends readonly unknown[], T>(
    call: (...args: Args) => Promise<T>,
    options?: RequestOptions,
  ): Request<Args, T>;
  poller<T>(call: () => Promise<T>, options: PollOptions<T>): Poller<T>;
  errorChannel(): CommandErrorChannel;
  subscribe<T>(source: Observable<T>, observer: (value: T) => void): Subscription;
}

/**
 * The scope as its creator sees it: the factories, plus the one way to end them.
 *
 * `close` is here and not on `AppScope` because a handler able to close the scope
 * it was handed could end its own app from the inside, leaving the container to
 * tear down a handle that is already dead.
 */
export interface ScopedAppResources extends AppScope {
  close(): void;
}

/**
 * A scope with nothing in it yet.
 *
 * Resources are disposed in reverse registration order. Registration order is
 * dependency order — an observer is registered after the source it observes — so
 * reversing it closes observers before their sources, and no disposal delivers a
 * value to something already gone.
 */
export function createAppScope(): ScopedAppResources {
  const disposers: Array<() => void> = [];
  let closed = false;

  /**
   * Take ownership of a freshly built resource.
   *
   * A resource built after `close` is disposed immediately rather than refused:
   * refusing would mean a factory that sometimes returns nothing, and handing back
   * a live resource nothing owns is the leak this whole type exists to prevent.
   */
  const own = <T>(resource: T, dispose: (owned: T) => void): T => {
    if (closed) {
      dispose(resource);
      return resource;
    }
    disposers.push(() => dispose(resource));
    return resource;
  };

  return {
    request<Args extends readonly unknown[], T>(
      call: (...args: Args) => Promise<T>,
      options?: RequestOptions,
    ): Request<Args, T> {
      return own(createRequest(call, options), owned => owned.teardown());
    },

    poller<T>(call: () => Promise<T>, options: PollOptions<T>): Poller<T> {
      return own(createPoller(call, options), owned => owned.teardown());
    },

    errorChannel(): CommandErrorChannel {
      return own(createCommandErrorChannel(), owned => owned.teardown());
    },

    subscribe<T>(source: Observable<T>, observer: (value: T) => void): Subscription {
      // The one factory whose construction has an effect of its own. `own` disposes
      // of a resource built after `close`, which is enough for the other three —
      // none of their constructors invoke anything. Subscribing does: every store
      // here is `BehaviorSubject`-backed and delivers its current value
      // synchronously, so going through `own` would run the observer once — writing
      // app state from a torn-down app — and unsubscribe immediately afterwards.
      // Refusing to subscribe at all is the only way not to deliver that value.
      if (closed) {
        const alreadyClosed = new Subscription();
        alreadyClosed.unsubscribe();
        return alreadyClosed;
      }
      return own(source.subscribe(observer), owned => owned.unsubscribe());
    },

    close(): void {
      closed = true;
      for (let index = disposers.length - 1; index >= 0; index -= 1) {
        disposers[index]();
      }
      disposers.length = 0;
    },
  };
}
