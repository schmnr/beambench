import { useTranslation } from 'react-i18next';
import { useProjectStore } from '../../stores/projectStore';
import type { Layer, ProjectObject } from '../../types/project';
import { INSPECTOR_HELP_TEXT_CLASS } from '../shared/panelAppearance';
import { displayLayerName, layerOperation } from '../layers/layerNaming';

const MIXED_LAYER_VALUE = '__mixed_layer__';

function isRasterObject(object: ProjectObject): boolean {
  return object.data.type === 'raster_image';
}

function layerAcceptsSelection(layer: Layer, objects: ProjectObject[]): boolean {
  if (layer.is_tool_layer) return true;
  const selectionKinds = new Set(objects.map(isRasterObject));
  if (selectionKinds.size !== 1) return false;
  const imageLayer = layer.entries[0]?.operation === 'image';
  return selectionKinds.has(true) ? imageLayer : !imageLayer;
}

interface SelectionLayerControlProps {
  objects: ProjectObject[];
}

export function SelectionLayerControl({ objects }: SelectionLayerControlProps) {
  const { t } = useTranslation();
  const project = useProjectStore((state) => state.project);
  const reassignLayer = useProjectStore((state) => state.reassignLayer);

  if (!project || objects.length === 0) return null;

  const objectIds = objects.map((object) => object.id);
  const layerIds = [...new Set(objects.map((object) => object.layer_id))];
  const selectedLayerId = layerIds.length === 1 ? layerIds[0] : MIXED_LAYER_VALUE;
  const compatibleLayers = project.layers.filter((layer) => layerAcceptsSelection(layer, objects));
  const selectedLayer = project.layers.find((layer) => layer.id === selectedLayerId) ?? null;

  const operationLabel = (layer: Layer) => t(
    `panels.machine.material_library.operation.${layerOperation(layer)}`,
  );

  return (
    <div className="flex flex-col gap-1.5" data-testid="selection-layer-control">
      <label className="flex items-center justify-between gap-3 text-xs text-bb-text-muted">
        <span>{t('panels.properties.layer')}</span>
        <span className="flex min-w-0 flex-1 items-center gap-2">
          <span
            aria-hidden="true"
            className="h-2.5 w-2.5 shrink-0 rounded-full border border-bb-border"
            style={{ backgroundColor: selectedLayer?.color_tag ?? 'transparent' }}
          />
          <select
            aria-label={t('panels.properties.layer')}
            className="min-w-0 flex-1 rounded border border-bb-border bg-bb-input px-2 py-1 text-xs text-bb-text"
            value={selectedLayerId}
            onChange={(event) => {
              if (event.target.value === MIXED_LAYER_VALUE) return;
              void reassignLayer(objectIds, event.target.value);
            }}
          >
            {selectedLayerId === MIXED_LAYER_VALUE && (
              <option value={MIXED_LAYER_VALUE} disabled>
                {t('panels.properties.mixed')}
              </option>
            )}
            {compatibleLayers.map((layer) => (
              <option key={layer.id} value={layer.id}>
                {displayLayerName(layer)} · {operationLabel(layer)}
              </option>
            ))}
          </select>
        </span>
      </label>
      <div className={INSPECTOR_HELP_TEXT_CLASS} data-property-helper>
        {t('panels.properties.layer_assignment_help')}
      </div>
    </div>
  );
}
