import { render, screen, fireEvent } from '@testing-library/react';
import { describe, expect, it } from 'bun:test';

import { withEvents, useEventObserver, useAdaptedEventObserver, type RC } from './index';

interface PanelEvents {
  panelAction: { source: string };
}
interface WidgetEvents {
  widgetClick: { widgetId: string };
}

const WidgetImpl: RC<object, WidgetEvents> = ({ events }) => (
  <button
    type='button'
    data-testid='widget-click'
    onClick={() => events.next({ type: 'widgetClick', payload: { widgetId: 'w-1' } })}
  >
    widget
  </button>
);

const Widget = withEvents<object, WidgetEvents>(WidgetImpl);

const PanelImpl: RC<{ title: string }, PanelEvents> = ({ events, title }) => {
  const widgetObserver = useAdaptedEventObserver<PanelEvents, WidgetEvents>(events, envelope => {
    if (envelope.type === 'widgetClick') {
      return { type: 'panelAction', payload: { source: envelope.payload.widgetId } };
    }
    return null;
  });

  return (
    <section>
      <h2>{title}</h2>
      <Widget eventObserver={widgetObserver} />
    </section>
  );
};

const Panel = withEvents<{ title: string }, PanelEvents>(PanelImpl);

describe('tddy-react-commons/events adapter acceptance', () => {
  it('forwards adapted child events to parent RC events observer', () => {
    const panelActions: Array<{ source: string }> = [];

    const Root = () => {
      const observer = useEventObserver<PanelEvents>({
        panelAction: payload => panelActions.push(payload),
      });
      return <Panel title='panel' eventObserver={observer} />;
    };

    render(<Root />);
    fireEvent.click(screen.getByTestId('widget-click'));

    expect(panelActions).toEqual([{ source: 'w-1' }]);
  });
});
