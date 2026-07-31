import { CircularArrayPropertiesSection } from '../dialogs/CircularArrayDialog';
import { GridArrayPropertiesSection } from '../dialogs/GridArrayDialog';
import { OffsetPropertiesSection } from '../dialogs/OffsetDialog';
import { useUiStore } from '../../stores/uiStore';

export function ModifierPropertiesSection() {
  const session = useUiStore((state) => state.modifierPropertiesSession);
  const close = useUiStore((state) => state.closeModifierProperties);

  if (!session) return null;

  switch (session.kind) {
    case 'offset':
      return <OffsetPropertiesSection objectIds={session.objectIds} onClose={close} />;
    case 'grid_array':
      return <GridArrayPropertiesSection objectIds={session.objectIds} onClose={close} />;
    case 'circular_array':
      return <CircularArrayPropertiesSection objectIds={session.objectIds} onClose={close} />;
  }
}
