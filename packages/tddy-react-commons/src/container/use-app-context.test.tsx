import { render } from '@testing-library/react';
import { type FC } from 'react';
import { afterEach, describe, expect, it, vi } from 'bun:test';

import { ReactAppContainer, useAppContext, type AppHandle, type AppHandler } from './index';

interface ProbeContext {
  marker: string;
}

class ProbeHandler implements AppHandler<ProbeContext> {
  teardownCount = 0;

  start(): AppHandle<ProbeContext> {
    return {
      context: { marker: 'ctx' },
      teardown: () => {
        this.teardownCount += 1;
      },
    };
  }
}

afterEach(() => {
  vi.restoreAllMocks();
});

describe('useAppContext', () => {
  it('throws when used outside a container (AT4)', () => {
    vi.spyOn(console, 'error').mockImplementation(() => {});
    // React re-throws through jsdom's error event; suppress the duplicate report so a
    // deliberately thrown error cannot be mistaken for a failure in the run output.
    const suppressUncaught = (event: ErrorEvent) => event.preventDefault();
    window.addEventListener('error', suppressUncaught);

    const Orphan: FC = () => {
      useAppContext<ProbeContext>();
      return null;
    };

    try {
      expect(() => render(<Orphan />)).toThrow(/ReactAppContainer/);
    } finally {
      window.removeEventListener('error', suppressUncaught);
    }
  });

  it('returns the handle context value (AT4)', () => {
    let seen: ProbeContext | undefined;

    const Probe: FC = () => {
      seen = useAppContext<ProbeContext>();
      return null;
    };

    render(ReactAppContainer.create(new ProbeHandler(), <Probe />));

    expect(seen).toEqual({ marker: 'ctx' });
  });

  it('does not expose teardown to the view layer (AT4)', () => {
    let seen: unknown;

    const Probe: FC = () => {
      seen = useAppContext<ProbeContext>();
      return null;
    };

    const handler = new ProbeHandler();
    render(ReactAppContainer.create(handler, <Probe />));

    expect(seen).not.toHaveProperty('teardown');
    expect(handler.teardownCount).toBe(0);
  });

  it('reads context from arbitrarily deep in the tree', () => {
    let seen: ProbeContext | undefined;

    const Leaf: FC = () => {
      seen = useAppContext<ProbeContext>();
      return null;
    };
    const Middle: FC = () => <Leaf />;
    const Outer: FC = () => <Middle />;

    render(ReactAppContainer.create(new ProbeHandler(), <Outer />));

    expect(seen?.marker).toBe('ctx');
  });
});
