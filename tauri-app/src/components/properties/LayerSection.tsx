import { useTranslation } from 'react-i18next';
import { useProjectStore } from '../../stores/projectStore';
import { projectService } from '../../services/projectService';
import { useNotificationStore } from '../../stores/notificationStore';
import { wrapBackendError } from '../../i18n/errors';
import { Select } from '../shared/Select';
import { Toggle } from '../shared/Toggle';

/**
 * Layer block for the Properties panel: a color-dot row assigning the
 * selection to a layer (the old palette's one-click behavior), the layer
 * picker, and the layer-level Output / Show / Air toggles from the
 * Cuts/Layers panel.
 */
export function LayerSection() {
  const { t } = useTranslation();
  const project = useProjectStore((s) => s.project);
  const selectedObjectIds = useProjectStore((s) => s.selectedObjectIds);
  const reassignLayer = useProjectStore((s) => s.reassignLayer);
  const updateLayer = useProjectStore((s) => s.updateLayer);
  const loadProject = useProjectStore((s) => s.loadProject);

  if (!project || selectedObjectIds.length === 0) return null;

  const layers = project.layers;
  const selectedObjects = project.objects.filter((o) => selectedObjectIds.includes(o.id));
  const layerIds = new Set(selectedObjects.map((o) => o.layer_id));
  const commonLayerId = layerIds.size === 1 ? [...layerIds][0] : '';
  const currentLayer = layers.find((l) => l.id === commonLayerId) ?? null;
  const primaryEntry = currentLayer?.entries[0];

  const notifyError = (error: unknown) => {
    useNotificationStore.getState().push(wrapBackendError(String(error)), 'error');
  };

  const handleShowToggle = async (visible: boolean) => {
    if (!currentLayer) return;
    try {
      await projectService.setLayerVisible(currentLayer.id, visible);
      await loadProject({ invalidatePreview: true });
    } catch (error) {
      notifyError(error);
    }
  };

  const handleAirToggle = async (airAssist: boolean) => {
    if (!currentLayer) return;
    try {
      await projectService.setLayerAirAssist(currentLayer.id, airAssist);
      await loadProject({ invalidatePreview: true });
    } catch (error) {
      notifyError(error);
    }
  };

  return (
    <div className="border-b border-bb-border pb-3 mb-1">
      <div className="flex items-center justify-between py-2">
        <span className="text-[10px] font-semibold tracking-wider text-bb-text-muted uppercase">
          {t('panels.properties.layer')}
        </span>
      </div>

      {/* Color-dot assignment row */}
      <div className="flex flex-wrap items-center gap-1.5">
        {layers.map((layer) => (
          <button
            key={layer.id}
            onClick={() => void reassignLayer(selectedObjectIds, layer.id)}
            title={layer.name}
            aria-label={layer.name}
            aria-pressed={layer.id === commonLayerId}
            className={`h-4 w-4 rounded ${
              layer.id === commonLayerId
                ? 'ring-2 ring-bb-accent ring-offset-1 ring-offset-bb-panel'
                : 'hover:ring-1 hover:ring-bb-control-border'
            }`}
            style={{ backgroundColor: layer.color_tag }}
          />
        ))}
      </div>

      {/* Layer picker (for long lists / mixed selections) */}
      <div className="mt-2">
        <Select
          label={t('panels.properties.layer')}
          value={commonLayerId}
          options={[
            ...(layerIds.size > 1 ? [{ value: '', label: t('panels.properties.mixed') }] : []),
            ...layers.map((l) => ({ value: l.id, label: l.name })),
          ]}
          onChange={(layer_id) => { if (layer_id) void reassignLayer(selectedObjectIds, layer_id); }}
        />
      </div>

      {/* Layer-level toggles (hidden for tool layers / mixed selections) */}
      {currentLayer && !currentLayer.is_tool_layer && (
        <div className="mt-2 flex flex-col gap-1.5">
          <Toggle
            label={t('panels.layers.header.output')}
            checked={currentLayer.enabled}
            onChange={(enabled) => void updateLayer(currentLayer.id, { enabled })}
          />
          <Toggle
            label={t('panels.layers.header.show')}
            checked={currentLayer.visible !== false}
            onChange={(visible) => void handleShowToggle(visible)}
          />
          <Toggle
            label={t('panels.layers.header.air')}
            checked={primaryEntry?.air_assist === true}
            onChange={(airAssist) => void handleAirToggle(airAssist)}
          />
        </div>
      )}
    </div>
  );
}
