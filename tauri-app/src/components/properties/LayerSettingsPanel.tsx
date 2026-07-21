import { useTranslation } from 'react-i18next';
import { useProjectStore } from '../../stores/projectStore';
import { TextInput } from '../shared/TextInput';
import { Toggle } from '../shared/Toggle';
import { SubLayerStack } from './SubLayerStack';

export function LayerSettingsPanel() {
  const { t } = useTranslation();
  const project = useProjectStore((s) => s.project);
  const selectedLayerId = useProjectStore((s) => s.selectedLayerId);
  const updateLayer = useProjectStore((s) => s.updateLayer);

  const selectedLayer = project?.layers.find((layer) => layer.id === selectedLayerId) ?? null;

  if (!selectedLayer) {
    return <div className="px-2 text-xs italic text-bb-text-dim">{t('panels.layer_settings.select_layer')}</div>;
  }

  const entryCount = selectedLayer.entries?.length ?? 1;

  return (
    <div className="flex flex-col gap-3 px-2">
      <div className="flex flex-col gap-2 rounded border border-bb-border bg-bb-bg-alt/60 p-2">
        <TextInput
          label={t('panels.layer_settings.name')}
          value={selectedLayer.name}
          onChange={(name) => void updateLayer(selectedLayer.id, { name })}
        />
        <Toggle
          label={t('panels.layer_settings.enabled')}
          checked={selectedLayer.enabled}
          onChange={(enabled) => void updateLayer(selectedLayer.id, { enabled })}
        />
        <Toggle
          label={t('panels.layer_settings.visible')}
          checked={selectedLayer.visible}
          onChange={(visible) => void updateLayer(selectedLayer.id, { visible })}
        />
        <div className="text-xs text-bb-text-dim">
          {selectedLayer.is_tool_layer
            ? t('panels.layer_settings.tool_layer')
            : entryCount > 1
            ? t('panels.layer_settings.sub_layers_count', { count: entryCount })
            : t('panels.layer_settings.single_sub_layer')}
        </div>
      </div>

      <SubLayerStack layerId={selectedLayer.id} />
    </div>
  );
}
