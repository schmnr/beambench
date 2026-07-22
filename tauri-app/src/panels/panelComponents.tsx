import type { ComponentType } from 'react';
import { useTranslation } from 'react-i18next';
import { useProjectStore } from '../stores/projectStore';
import { LayerList } from '../components/layers/LayerList';
import { PropertiesPanel } from '../components/properties/PropertiesPanel';
import { TextDefaultsSection } from '../components/properties/TextDefaultsSection';
import { useUiStore } from '../stores/uiStore';
import { MovePanel } from '../components/machine/MovePanel';
import { ConsoleWindow } from '../components/machine/ConsoleWindow';
import { MacrosWindow } from '../components/machine/MacrosWindow';
import { LaserPanel } from '../components/machine/LaserPanel';
import { MaterialLibrary } from '../components/machine/MaterialLibrary';
import { ColorPalette } from '../components/layout/ColorPalette';
import { CameraContent } from '../components/machine/CameraContent';
import { ArtLibraryPanel } from '../components/panels/ArtLibraryPanel';
import { ConnectionDiagnosticsPanel } from '../components/panels/ConnectionDiagnosticsPanel';
import { MeasurementPanel } from '../components/panels/MeasurementPanel';

function CutsLayersContent() {
  const { t } = useTranslation();
  const hasProject = useProjectStore((s) => s.project !== null);
  return hasProject ? <LayerList /> : <div className="text-xs text-bb-text-dim italic px-2 py-2">{t('panels.empty.no_project')}</div>;
}

function MoveContent() {
  return <MovePanel />;
}

function PropertiesContent() {
  const { t } = useTranslation();
  const hasProject = useProjectStore((s) => s.project !== null);
  const hasSelection = useProjectStore((s) => s.selectedObjectIds.length > 0);
  const textToolActive = useUiStore((s) => s.activeTool === 'text');
  if (!hasProject) {
    return <div className="text-xs text-bb-text-dim italic px-2 py-2">{t('panels.empty.nothing_selected')}</div>;
  }
  // Text tool armed with nothing selected: edit the defaults for the next
  // text object (previously lived in the properties toolbar).
  if (!hasSelection && textToolActive) {
    return <TextDefaultsSection />;
  }
  return <PropertiesPanel />;
}

function ArtLibraryContent() {
  return <ArtLibraryPanel />;
}

/**
 * Authoritative map from panel ID to React component.
 * Every panel in PANEL_REGISTRY must have an entry here.
 */
export const PANEL_COMPONENTS: Record<string, ComponentType> = {
  cuts_layers: CutsLayersContent,
  move: MoveContent,
  console: ConsoleWindow,
  macros: MacrosWindow,
  properties: PropertiesContent,
  measurement: MeasurementPanel,
  laser: LaserPanel,
  material: MaterialLibrary,
  color_palette: ColorPalette,
  camera: CameraContent,
  art_library: ArtLibraryContent,
  connection_diagnostics: ConnectionDiagnosticsPanel,
};
