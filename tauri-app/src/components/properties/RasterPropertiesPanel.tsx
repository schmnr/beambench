import { useTranslation } from 'react-i18next';
import { ImageDown, ScanLine, SlidersHorizontal } from 'lucide-react';
import { useProjectStore } from '../../stores/projectStore';
import { executeAppCommand } from '../../commands/appCommands';
import { APP_COMMANDS } from '../../commands/appCommandIds';
import type { ObjectData } from '../../types/project';
import { INSPECTOR_SECTION_HEADER_CLASS } from '../shared/panelAppearance';

interface RasterPropertiesPanelProps {
  objectId: string;
  data: Extract<ObjectData, { type: 'raster_image' }>;
}

export function RasterPropertiesPanel({ objectId, data }: RasterPropertiesPanelProps) {
  const { t } = useTranslation();
  const project = useProjectStore((s) => s.project);
  const setImageMaskPolarity = useProjectStore((s) => s.setImageMaskPolarity);
  const removeImageMask = useProjectStore((s) => s.removeImageMask);
  const masks = data.masks ?? [];
  const imageTitle = t('panels.layers.mode.image');

  return (
    <div className="flex flex-col gap-1.5 pt-1 border-t border-bb-border">
      <div className={INSPECTOR_SECTION_HEADER_CLASS}>{imageTitle}</div>
      <div className="grid grid-cols-2 gap-1.5" role="group" aria-label={imageTitle}>
        <button
          type="button"
          className="flex h-8 items-center justify-center gap-1.5 rounded-lg border border-bb-border bg-bb-surface px-2 text-[11px] font-medium text-bb-text outline-none transition-colors hover:bg-bb-hover focus-visible:ring-1 focus-visible:ring-bb-accent"
          onClick={() => void executeAppCommand(APP_COMMANDS.TOOLS_ADJUST_IMAGE)}
        >
          <SlidersHorizontal size={14} className="shrink-0 text-bb-accent" />
          {t('menus.tools.adjust_image')}
        </button>
        <button
          type="button"
          className="flex h-8 items-center justify-center gap-1.5 rounded-lg border border-bb-border bg-bb-surface px-2 text-[11px] font-medium text-bb-text outline-none transition-colors hover:bg-bb-hover focus-visible:ring-1 focus-visible:ring-bb-accent"
          onClick={() => void executeAppCommand(APP_COMMANDS.TOOLS_TRACE_IMAGE)}
        >
          <ScanLine size={14} className="shrink-0 text-bb-accent" />
          {t('menus.tools.trace_image')}
        </button>
        <button
          type="button"
          className="col-span-2 flex h-8 items-center justify-center gap-1.5 rounded-lg border border-bb-border bg-bb-surface px-2 text-[11px] font-medium text-bb-text outline-none transition-colors hover:bg-bb-hover focus-visible:ring-1 focus-visible:ring-bb-accent"
          onClick={() => void executeAppCommand(APP_COMMANDS.FILE_SAVE_PROCESSED_BITMAP)}
        >
          <ImageDown size={14} className="shrink-0 text-bb-accent" />
          {t('menus.file.export_processed_image')}
        </button>
      </div>
      {masks.length > 0 && (
        <div className="rounded border border-bb-border bg-bb-surface-2 px-2 py-1.5">
          <div className="flex items-center justify-between text-xs">
            <span className="font-medium text-bb-text">{t('panels.raster_properties.masks_count', { count: masks.length })}</span>
            <button
              type="button"
              className="text-[10px] text-bb-accent hover:underline"
              onClick={() => void removeImageMask(objectId)}
            >
              {t('panels.raster_properties.clear')}
            </button>
          </div>
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
                    type="button"
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
        </div>
      )}
    </div>
  );
}
