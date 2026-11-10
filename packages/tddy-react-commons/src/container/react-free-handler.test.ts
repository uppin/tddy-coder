import { BehaviorSubject, Subject, finalize, from, switchMap, tap } from 'rxjs';
import { describe, expect, it } from 'bun:test';

import { createStore, type Store } from '../stores/index';

import * as appLib from './index';
import type { AppHandle, AppHandler } from './index';

/**
 * AT1 — a handler is pure rxjs business logic. This module imports NO React and
 * NO testing-library: if this file ever needs them, indirection of control is broken.
 */

interface DocState {
  assetId: string | null;
  pageCount: number;
  currentPage: number;
  selectPage: (page: number) => void;
}

interface EditorAppContext {
  docStore: Store<DocState>;
  isLoading$: BehaviorSubject<boolean>;
}

interface EditorDeps {
  loadDocument: (assetId: string) => Promise<{ pageCount: number }>;
}

class EditorAppHandler implements AppHandler<EditorAppContext> {
  private readonly loadRequest$ = new Subject<string>();
  private readonly isLoading$ = new BehaviorSubject(false);

  private readonly docStore = createStore<DocState>(set => ({
    assetId: null,
    pageCount: 0,
    currentPage: 1,
    selectPage: (page: number) => set({ currentPage: page }),
  }));

  constructor(private readonly deps: EditorDeps) {}

  start(): AppHandle<EditorAppContext> {
    const subscription = this.loadRequest$
      .pipe(
        tap(() => this.isLoading$.next(true)),
        switchMap(assetId =>
          from(this.deps.loadDocument(assetId)).pipe(
            tap(({ pageCount }) => this.docStore.setState({ pageCount })),
            finalize(() => this.isLoading$.next(false)),
          ),
        ),
      )
      .subscribe();

    return {
      context: { docStore: this.docStore, isLoading$: this.isLoading$ },
      teardown: () => subscription.unsubscribe(),
    };
  }

  requestLoad(assetId: string): void {
    this.docStore.setState({ assetId });
    this.loadRequest$.next(assetId);
  }
}

describe('app handler without React', () => {
  it('requires no runtime import from the library — handler contract is types only', () => {
    // Everything with a runtime presence here is React-side. `AppHandler` / `AppHandle` are
    // types, so a handler pays nothing at runtime for implementing the contract.
    expect(Object.keys(appLib).sort()).toEqual([
      'ReactAppContainer',
      'createSliceHook',
      'useAppContext',
    ]);
  });

  it('exposes context only through start()', () => {
    const handler = new EditorAppHandler({
      loadDocument: async () => ({ pageCount: 7 }),
    });

    const handle = handler.start();

    expect(handle.context.docStore.getState().pageCount).toBe(0);
    expect(typeof handle.teardown).toBe('function');
  });

  it('runs its rxjs pipeline when driven directly, with no React render', async () => {
    const handler = new EditorAppHandler({
      loadDocument: async assetId => ({ pageCount: assetId === 'asset-1' ? 7 : 0 }),
    });
    const { context, teardown } = handler.start();

    const loadingStates: boolean[] = [];
    context.isLoading$.subscribe(value => loadingStates.push(value));

    handler.requestLoad('asset-1');
    await Promise.resolve();

    expect(context.docStore.getState().assetId).toBe('asset-1');
    expect(context.docStore.getState().pageCount).toBe(7);
    expect(loadingStates).toEqual([false, true, false]);

    teardown();
  });

  it('stops reacting to commands after teardown', async () => {
    const handler = new EditorAppHandler({
      loadDocument: async () => ({ pageCount: 3 }),
    });
    const { context, teardown } = handler.start();

    teardown();
    handler.requestLoad('asset-2');
    await Promise.resolve();

    expect(context.docStore.getState().pageCount).toBe(0);
  });

  it('applies store actions colocated in the handler', () => {
    const handler = new EditorAppHandler({
      loadDocument: async () => ({ pageCount: 1 }),
    });
    const { context } = handler.start();

    context.docStore.getState().selectPage(4);

    expect(context.docStore.getState().currentPage).toBe(4);
  });
});
