import { useState } from 'react';
import { useTranslation } from 'react-i18next';
import { useProjectStore } from '../../stores/projectStore';
import { NumberInput } from '../shared/NumberInput';
import { Toggle } from '../shared/Toggle';
import { CutSettingsEditor } from '../layers/CutSettingsEditor';
import type { ObjectData, RasterAdjustments } from '../../types/project';

interface RasterPropertiesPanelProps {
  objectId: string;
  data: Extract<ObjectData, { type: 'raster_image' }>;
  passThrough?: boolean;
}

const DEFAULT_ADJUSTMENTS: RasterAdjustments = {
  brightness: 0,
  contrast: 0,
  gamma: 1,
  invert: false,
  threshold: 128,
  saturation: 1,
  sharpen: 0,
  edge_enhance: false,
  enhance_radius: 0,
  enhance_amount: 0,
  enhance_denoise: 0,
};

export function RasterPropertiesPanel({ objectId, data, passThrough }: RasterPropertiesPanelProps) {
  const { t } = useTranslation();
  const updateObjectData = useProjectStore((s) => s.updateObjectData);
  const project = useProjectStore((s) => s.project);
  const setImageMaskPolarity = useProjectStore((s) => s.setImageMaskPolarity);
  const removeImageMask = useProjectStore((s) => s.removeImageMask);
  const layerId = useProjectStore(
    (s) => s.project?.objects.find((o) => o.id === objectId)?.layer_id ?? null,
  );
  const [editingLayer, setEditingLayer] = useState(false);
  const adj = data.adjustments ?? DEFAULT_ADJUSTMENTS;
  const masks = data.masks ?? [];

  const update = (patch: Partial<RasterAdjustments>) => {
    updateObjectData(objectId, { ...data, adjustments: { ...adj, ...patch } });
  };

  return (
    <div className="flex flex-col gap-1.5 pt-1 border-t border-bb-border">
      <div className="flex items-center justify-between">
        <div className="text-xs font-medium text-bb-accent uppercase tracking-wider">{t('panels.raster_properties.title')}</div>
        {layerId && (
          <button
            data-testid="edit-layer-settings"
            className="text-[10px] text-bb-accent hover:underline"
            onClick={() => setEditingLayer(true)}
            title={t('panels.raster_properties.edit_layer_settings_title')}
          >
            {t('panels.raster_properties.edit_layer_settings')}
          </button>
        )}
      </div>
      {editingLayer && layerId && (
        <CutSettingsEditor layerId={layerId} onClose={() => setEditingLayer(false)} />
      )}
      {passThrough && (
        <div className="text-[10px] text-bb-text-dim italic">{t('panels.raster_properties.pass_through_notice')}</div>
      )}
      <div className="rounded border border-bb-border bg-bb-surface-2 px-2 py-1.5">
        <div className="flex items-center justify-between text-xs">
          <span className="font-medium text-bb-text">{t('panels.raster_properties.masks_count', { count: masks.length })}</span>
          {masks.length > 0 && (
            <button
              className="text-[10px] text-bb-accent hover:underline"
              onClick={() => void removeImageMask(objectId)}
            >
              {t('panels.raster_properties.clear')}
            </button>
          )}
        </div>
        {masks.length > 0 && (
          <div className="mt-1 flex flex-col gap-1">
            {masks.map((mask) => {
              const maskObject = project?.objects.find((o) => o.id === mask.object_id);
              return (
                <div key={mask.object_id} className="flex items-center gap-1 text-[11px]">
                  <span className="min-w-0 flex-1 truncate text-bb-text-dim">
                    {maskObject?.name ?? t('panels.raster_properties.missing_mask')}
                  </span>
                  <select
                    className="bg-bb-surface border border-bb-border rounded px-1 py-0.5 text-bb-text"
                    value={mask.polarity}
                    onChange={(event) => {
                      void setImageMaskPolarity(objectId, mask.object_id, event.target.value as typeof mask.polarity);
                    }}
                  >
                    <option value="keep_inside">{t('panels.raster_properties.keep_inside')}</option>
                    <option value="keep_outside">{t('panels.raster_properties.keep_outside')}</option>
                  </select>
                  <button
                    className="px-1 text-bb-text-dim hover:text-bb-text"
                    onClick={() => void removeImageMask(objectId, mask.object_id)}
                    title={t('panels.raster_properties.remove_mask_title')}
                  >
                    {t('panels.raster_properties.remove')}
                  </button>
                </div>
              );
            })}
          </div>
        )}
      </div>
      {!passThrough && (
        <>
          <NumberInput
            label={t('panels.raster_properties.brightness')}
            value={adj.brightness}
            onChange={(brightness) => update({ brightness })}
            min={-1}
            max={1}
            step={0.05}
          />
          <NumberInput
            label={t('panels.raster_properties.contrast')}
            value={adj.contrast}
            onChange={(contrast) => update({ contrast })}
            min={-1}
            max={1}
            step={0.05}
          />
          <NumberInput
            label={t('panels.raster_properties.gamma')}
            value={adj.gamma}
            onChange={(gamma) => update({ gamma })}
            min={0.1}
            max={3.0}
            step={0.1}
          />
        </>
      )}
      <NumberInput
        label={t('panels.raster_properties.threshold')}
        value={adj.threshold}
        onChange={(threshold) => update({ threshold })}
        min={0}
        max={255}
        step={1}
      />
      {!passThrough && (
        <>
          <NumberInput
            label={t('panels.raster_properties.saturation')}
            value={adj.saturation}
            onChange={(saturation) => update({ saturation })}
            min={0}
            max={2}
            step={0.1}
          />
          <NumberInput
            label={t('panels.raster_properties.sharpen')}
            value={adj.sharpen ?? 0}
            onChange={(sharpen) => update({ sharpen })}
            min={0}
            max={2}
            step={0.1}
          />
          <Toggle
            label={t('panels.raster_properties.invert')}
            checked={adj.invert}
            onChange={(invert) => update({ invert })}
          />
          <Toggle
            label={t('panels.raster_properties.edge_enhance')}
            checked={adj.edge_enhance ?? false}
            onChange={(edge_enhance) => update({ edge_enhance })}
          />
        </>
      )}
    </div>
  );
}
