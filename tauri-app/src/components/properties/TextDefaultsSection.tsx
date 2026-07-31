import { useTranslation } from 'react-i18next';
import { MousePointer2, Type } from 'lucide-react';
import { useUiStore } from '../../stores/uiStore';
import type { TextLayoutMode } from '../../types/project';
import { TextControls, type TextControlValue } from './TextControls';

/** Settings for the next text object, shown before anything is placed. */
export function TextDefaultsSection() {
  const { t } = useTranslation();
  const textDefaults = useUiStore((state) => state.textDefaults);
  const updateTextDefaults = useUiStore((state) => state.updateTextDefaults);

  const value: TextControlValue = {
    ...textDefaults,
    max_width: textDefaults.max_width ?? null,
    squeeze: textDefaults.squeeze ?? false,
    rtl: textDefaults.rtl ?? false,
  };

  return (
    <div className="flex flex-col gap-2.5 px-3 py-2.5">
      <div className="flex items-center gap-2 text-xs font-semibold text-bb-text">
        <Type size={15} className="text-bb-accent" />
        {t('panels.text_properties.tool_title')}
      </div>
      <div className="flex gap-2 rounded bg-bb-bg px-2.5 py-2 text-[11px] leading-4 text-bb-text-muted">
        <MousePointer2 size={14} className="mt-0.5 shrink-0 text-bb-accent" />
        <span>{t('panels.text_properties.creation_hint')}</span>
      </div>
      <TextControls
        value={value}
        creationMode
        onPatch={(patch) => updateTextDefaults(patch)}
        onLayoutChange={(layoutMode) => {
          const layout_mode = layoutMode as TextLayoutMode;
          updateTextDefaults({
            layout_mode,
            on_path: layout_mode === 'path',
            ...(layout_mode === 'bend' && textDefaults.bend_radius === 0 ? { bend_radius: 50 } : {}),
          });
        }}
      />
    </div>
  );
}
