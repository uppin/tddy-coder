/**
 * DOM for `bun test`: the hook and component suites render through Testing Library, which needs
 * `document` and `window`. Registered before any suite imports React.
 *
 * Testing Library's async-utility budget is independent of the runner's per-test timeout and
 * defaults to 1 s; it is aligned to 5 s so a busy CI worker reports a timeout as a timeout, rather
 * than as whichever assertion happened to be waiting.
 */
import { GlobalRegistrator } from "@happy-dom/global-registrator";

GlobalRegistrator.register();

const { configure } = await import("@testing-library/react");
configure({ asyncUtilTimeout: 5000 });
