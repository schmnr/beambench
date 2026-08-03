import { PanelColumn } from './PanelColumn';

const RIGHT_COLUMN = 'right' as const;

export function RightPanel() {
  return <PanelColumn side={RIGHT_COLUMN} showMachineProfile />;
}
