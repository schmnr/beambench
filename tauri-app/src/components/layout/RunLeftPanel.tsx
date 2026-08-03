import { PanelColumn } from './PanelColumn';

const LEFT_COLUMN = 'left' as const;

/**
 * Run-mode left panel: a normal floating panel card (like the right one)
 * hosting jog/move; camera, macros, and console live under the laser panel.
 */
export function RunLeftPanel() {
  return <PanelColumn side={LEFT_COLUMN} />;
}
