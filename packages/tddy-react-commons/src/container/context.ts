import { createContext } from 'react';

/**
 * Internal module shared by `container` and `testkit`. Not part of either subpath's public
 * API — the raw context stays private so the only sanctioned ways to provide it are
 * `ReactAppContainer.create` in production and `AppContextStub` in tests.
 */

export const MISSING_CONTEXT = Symbol('tddy-react-commons/container:missing-context');

export const AppContext = createContext<unknown>(MISSING_CONTEXT);
