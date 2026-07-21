import { useTranslation } from 'react-i18next';
import { useProjectStore } from '../../stores/projectStore';
import { TextInput } from '../shared/TextInput';
import { NumberInput } from '../shared/NumberInput';
import { Select } from '../shared/Select';
import { ColorDot } from '../shared/ColorDot';
import { Toggle } from '../shared/Toggle';
import { TextPropertiesPanel } from './TextPropertiesPanel';
import { RasterPropertiesPanel } from './RasterPropertiesPanel';
import { vectorService } from '../../services/vectorService';
import { useNotificationStore } from '../../stores/notificationStore';
import { wrapBackendError } from '../../i18n/errors';
import { isTransformLocked, notifyTransformLocked } from '../../utils/transformLocks';

const TOAST_SUCCESS = 'success' as const;
const TOAST_ERROR = 'error' as const;
const TRANSFORM_LOCK_SCALE = 'scale' as const;

export function PropertiesPanel() {
  const { t } = useTranslation();
  const project = useProjectStore((s) => s.project);
  const selectedObjectIds = useProjectStore((s) => s.selectedObjectIds);
  const updateObject = useProjectStore((s) => s.updateObject);
  const updateObjectData = useProjectStore((s) => s.updateObjectData);
  const resizeShapeObject = useProjectStore((s) => s.resizeShapeObject);
  const loadProject = useProjectStore((s) => s.loadProject);
  const booleanPending = useProjectStore((s) => s.booleanPending);
  const reassignLayer = useProjectStore((s) => s.reassignLayer);
  const lockObjects = useProjectStore((s) => s.lockObjects);
  const unlockObjects = useProjectStore((s) => s.unlockObjects);
  const setObjectsVisible = useProjectStore((s) => s.setObjectsVisible);

  const selectedObject = project?.objects.find((o) => o.id === selectedObjectIds[0]) ?? null;
  const secondObject = selectedObjectIds.length === 2
    ? project?.objects.find((o) => o.id === selectedObjectIds[1]) ?? null
    : null;

  // Multi-selection: batch controls for 2+, boolean ops for exactly 2
  if (selectedObjectIds.length >= 2) {
    const selectedObjects = project?.objects.filter((o) => selectedObjectIds.includes(o.id)) ?? [];
    const layers = project?.layers ?? [];
    const layerOptions = layers.map((l) => ({ value: l.id, label: l.name }));

    // Compute mixed state
    const allLayerIds = new Set(selectedObjects.map((o) => o.layer_id));
    const commonLayerId = allLayerIds.size === 1 ? [...allLayerIds][0] : '';
    const allVisible = selectedObjects.every((o) => o.visible);
    const noneVisible = selectedObjects.every((o) => !o.visible);
    const allLocked = selectedObjects.every((o) => o.locked);
    const noneLocked = selectedObjects.every((o) => !o.locked);

    return (
      <div className="flex flex-col gap-2 px-2">
        <div className="text-xs text-bb-text-dim">{t('panels.properties.objects_selected', { count: selectedObjectIds.length })}</div>

        {/* Batch controls */}
        <Select
          label={t('panels.properties.layer')}
          value={commonLayerId}
          options={[
            ...(allLayerIds.size > 1 ? [{ value: '', label: t('panels.properties.mixed') }] : []),
            ...layerOptions,
          ]}
          onChange={(layer_id) => { if (layer_id) void reassignLayer(selectedObjectIds, layer_id); }}
        />
        <div className="flex gap-2 items-center text-xs">
          <label className="flex items-center gap-1">
            <input
              type="checkbox"
              data-testid="batch-visible"
              checked={allVisible}
              ref={(el) => { if (el) el.indeterminate = !allVisible && !noneVisible; }}
              onChange={() => void setObjectsVisible(selectedObjectIds, !allVisible)}
            />
            {t('panels.properties.visible')}
          </label>
          <label className="flex items-center gap-1">
            <input
              type="checkbox"
              data-testid="batch-locked"
              checked={allLocked}
              ref={(el) => { if (el) el.indeterminate = !allLocked && !noneLocked; }}
              onChange={() => void ((!allLocked ? lockObjects : unlockObjects)(selectedObjectIds))}
            />
            {t('panels.properties.locked')}
          </label>
        </div>

        {/* Boolean ops only for exactly 2 */}
        {selectedObjectIds.length === 2 && selectedObject && secondObject && (
          <div className="flex flex-col gap-1 mt-1">
            <div className="text-xs text-bb-text-dim">{t('panels.properties.boolean_operations')}</div>
            <button
              disabled={booleanPending}
              className={`w-full text-xs px-2 py-1 rounded bg-bb-surface-2 border border-bb-border text-bb-text ${booleanPending ? 'opacity-50 cursor-not-allowed' : 'hover:bg-bb-surface-3'}`}
              onClick={() => {
                useProjectStore.getState().booleanUnion(selectedObjectIds[0], selectedObjectIds[1]);
              }}
            >
              {t('panels.properties.union')}
            </button>
            <button
              disabled={booleanPending}
              className={`w-full text-xs px-2 py-1 rounded bg-bb-surface-2 border border-bb-border text-bb-text ${booleanPending ? 'opacity-50 cursor-not-allowed' : 'hover:bg-bb-surface-3'}`}
              onClick={() => {
                useProjectStore.getState().booleanSubtract(selectedObjectIds[0], selectedObjectIds[1]);
              }}
            >
              {t('panels.properties.subtract')}
            </button>
            <button
              disabled={booleanPending}
              className={`w-full text-xs px-2 py-1 rounded bg-bb-surface-2 border border-bb-border text-bb-text ${booleanPending ? 'opacity-50 cursor-not-allowed' : 'hover:bg-bb-surface-3'}`}
              onClick={() => {
                useProjectStore.getState().booleanIntersection(selectedObjectIds[0], selectedObjectIds[1]);
              }}
            >
              {t('panels.properties.intersection')}
            </button>
          </div>
        )}
      </div>
    );
  }

  if (!selectedObject) {
    return <div className="text-xs text-bb-text-dim italic px-2">{t('panels.properties.nothing_selected')}</div>;
  }

  const layers = project?.layers ?? [];
  const layerOptions = layers.map((l) => ({ value: l.id, label: l.name }));

  const x = selectedObject.bounds.min.x;
  const y = selectedObject.bounds.min.y;
  const w = selectedObject.bounds.max.x - selectedObject.bounds.min.x;
  const h = selectedObject.bounds.max.y - selectedObject.bounds.min.y;

  const updateDimensions = (newW: number, newH: number) => {
    void resizeShapeObject(selectedObject.id, {
      min: { x, y },
      max: { x: x + newW, y: y + newH },
    });
  };

  const canConvertToPath =
    selectedObject.data?.type === 'shape' ||
    selectedObject.data?.type === 'text' ||
    selectedObject.data?.type === 'polygon' ||
    selectedObject.data?.type === 'star';

  // Layer color for ColorDot
  const currentLayer = layers.find((l) => l.id === selectedObject.layer_id);
  const transformLocks = project?.transform_locks;

  // Corner radius: only for rectangle shapes
  const isRectangleShape = selectedObject.data?.type === 'shape' && selectedObject.data.kind === 'rectangle';
  const isEllipseShape = selectedObject.data?.type === 'shape' && selectedObject.data.kind === 'ellipse';
  const isPolygonShape = selectedObject.data?.type === 'polygon';
  const isStarShape = selectedObject.data?.type === 'star';
  const polygonData = isPolygonShape ? selectedObject.data as Extract<typeof selectedObject.data, { type: 'polygon' }> : null;
  const starData = isStarShape ? selectedObject.data as Extract<typeof selectedObject.data, { type: 'star' }> : null;

  return (
    <div className="flex flex-col gap-2 px-2">
      <TextInput
        label={t('panels.properties.name')}
        value={selectedObject.name}
        onChange={(name) => updateObject(selectedObject.id, { name })}
      />
      <div className="flex items-center gap-2">
        {currentLayer && <ColorDot color={currentLayer.color_tag} />}
        <div className="flex-1">
          <Select
            label={t('panels.properties.layer')}
            value={selectedObject.layer_id}
            options={layerOptions}
            onChange={(layer_id) => updateObject(selectedObject.id, { layer_id })}
          />
        </div>
      </div>

      <NumberInput
        label={t('panels.properties.power_scale_percent')}
        value={Math.round((selectedObject.power_scale ?? 1) * 100)}
        onChange={(v) => updateObject(selectedObject.id, { power_scale: v / 100 })}
        step={1}
        min={0}
        max={100}
      />

      <NumberInput
        label={t('panels.properties.cut_priority')}
        value={selectedObject.priority ?? 0}
        onChange={(v) => updateObject(selectedObject.id, { priority: v })}
        step={1}
        min={-99}
        max={99}
      />

      <Toggle
        label={t('panels.properties.locked')}
        checked={selectedObject.locked}
        onChange={(locked) => updateObject(selectedObject.id, { locked })}
      />

      {(isRectangleShape || isEllipseShape || isPolygonShape || isStarShape) && (
        <div className="flex flex-col gap-1.5 pt-1 border-t border-bb-border">
          <div className="text-xs font-medium text-bb-accent uppercase tracking-wider">{t('panels.properties.shape')}</div>
          <NumberInput
            label={t('panels.properties.width')}
            value={Math.round(w * 100) / 100}
            onChange={(newW) => {
              if (isTransformLocked(transformLocks, TRANSFORM_LOCK_SCALE)) {
                notifyTransformLocked(TRANSFORM_LOCK_SCALE);
                return;
              }
              if (selectedObject.lock_aspect_ratio && w > 0) {
                const ratio = h / w;
                updateDimensions(newW, newW * ratio);
              } else {
                updateDimensions(newW, h);
              }
            }}
            step={0.1}
            min={0.1}
          />
          <NumberInput
            label={t('panels.properties.height')}
            value={Math.round(h * 100) / 100}
            onChange={(newH) => {
              if (isTransformLocked(transformLocks, TRANSFORM_LOCK_SCALE)) {
                notifyTransformLocked(TRANSFORM_LOCK_SCALE);
                return;
              }
              if (selectedObject.lock_aspect_ratio && h > 0) {
                const ratio = w / h;
                updateDimensions(newH * ratio, newH);
              } else {
                updateDimensions(w, newH);
              }
            }}
            step={0.1}
            min={0.1}
          />
        </div>
      )}

      {isRectangleShape && selectedObject.data.type === 'shape' && (
        <NumberInput
          label={t('panels.properties.corner_radius')}
          value={selectedObject.data.corner_radius}
          onChange={(value) => {
            if (selectedObject.data.type === 'shape') {
              updateObjectData(selectedObject.id, { ...selectedObject.data, corner_radius: value });
            }
          }}
          step={0.5}
        />
      )}

      {polygonData && (
        <NumberInput
          label={t('panels.properties.sides')}
          value={polygonData.sides}
          onChange={(sides) => updateObjectData(selectedObject.id, { ...polygonData, sides: Math.max(3, Math.round(sides)) })}
          step={1}
          min={3}
        />
      )}

      {starData && (
        <div className="flex flex-col gap-1.5">
          <NumberInput
            label={t('panels.properties.points')}
            value={starData.points}
            onChange={(points) => updateObjectData(selectedObject.id, {
              ...starData,
              points: Math.max(3, Math.round(points)),
              corner_radii: [],
            })}
            step={1}
            min={3}
          />
          <NumberInput
            label={t('panels.properties.bulge')}
            value={starData.bulge}
            onChange={(bulge) => updateObjectData(selectedObject.id, { ...starData, bulge })}
            step={0.01}
            min={0}
            max={1}
          />
          <NumberInput
            label={t('panels.properties.ratio')}
            value={starData.ratio}
            onChange={(ratio) => updateObjectData(selectedObject.id, { ...starData, ratio })}
            step={0.01}
            min={0.05}
            max={0.95}
          />
          <label className="flex items-center gap-2 text-xs">
            <span className="text-bb-text-muted shrink-0">{t('panels.properties.dual_radius')}</span>
            <input
              type="checkbox"
              checked={starData.dual_radius}
              onChange={(e) => updateObjectData(selectedObject.id, {
                ...starData,
                dual_radius: e.target.checked,
                ratio2: e.target.checked ? (starData.ratio2 ?? 0.7) : null,
                corner_radii: [],
              })}
            />
          </label>
          {starData.dual_radius && (
            <NumberInput
              label={t('panels.properties.ratio_2')}
              value={starData.ratio2 ?? 0.7}
              onChange={(ratio2) => updateObjectData(selectedObject.id, { ...starData, ratio2 })}
              step={0.01}
              min={0.05}
              max={1}
            />
          )}
        </div>
      )}

      {selectedObject.data?.type === 'text' && (
        <TextPropertiesPanel objectId={selectedObject.id} data={selectedObject.data} />
      )}

      {selectedObject.data?.type === 'raster_image' && (
        <RasterPropertiesPanel
          objectId={selectedObject.id}
          data={selectedObject.data}
          passThrough={project?.layers.find((l) => l.id === selectedObject.layer_id)?.entries[0]?.raster_settings?.pass_through ?? false}
        />
      )}

      {canConvertToPath && (
        <button
          className="w-full text-xs px-2 py-1 rounded bg-bb-surface-2 border border-bb-border hover:bg-bb-surface-3 text-bb-text mt-1"
          onClick={() => {
            vectorService
              .convertToPath(selectedObject.id)
              .then(() => {
                loadProject({ invalidatePreview: true });
                useNotificationStore.getState().push(t('panels.properties.converted_to_path'), TOAST_SUCCESS);
              })
              .catch((err) => {
                useNotificationStore.getState().push(wrapBackendError(String(err)), TOAST_ERROR);
              });
          }}
        >
          {t('panels.properties.convert_to_path')}
        </button>
      )}
    </div>
  );
}
