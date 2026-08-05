import { cleanup, render, screen } from '@testing-library/react';
import { afterEach, describe, expect, it } from 'vitest';
import { PanelHost, usePanelHost } from '../PanelHost';

afterEach(cleanup);

function ContextProbe() {
  const host = usePanelHost();
  return (
    <div data-testid="context-probe">
      {host.panelTypeId}:{host.orientation}:{host.placement.kind}
    </div>
  );
}

describe('PanelHost', () => {
  it('marks bottom-docked panels as wide even at compact heights', () => {
    render(
      <PanelHost panelInstanceId="console::2" placement={{ kind: 'docked', zone: 'bottom' }}>
        <ContextProbe />
      </PanelHost>,
    );

    const host = screen.getByTestId('context-probe').parentElement;
    expect(host?.dataset.panelInstance).toBe('console::2');
    expect(host?.dataset.panelType).toBe('console');
    expect(host?.dataset.panelZone).toBe('bottom');
    expect(host?.dataset.panelOrientation).toBe('wide');
    expect(screen.getByTestId('context-probe').textContent).toBe('console:wide:docked');
  });

  it('keeps side panels in the vertical composition by default', () => {
    render(
      <PanelHost panelInstanceId="layers" placement={{ kind: 'docked', zone: 'top-right' }}>
        <ContextProbe />
      </PanelHost>,
    );

    expect(screen.getByTestId('context-probe').textContent).toBe('layers:vertical:docked');
  });
});
