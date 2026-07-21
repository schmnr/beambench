import { useEffect, useRef } from 'react';
import { createPortal } from 'react-dom';
import { useTranslation } from 'react-i18next';
import { useProjectStore } from '../../stores/projectStore';
import { SubLayerStack } from '../properties/SubLayerStack';
import { useFocusTrap } from '../../hooks/useFocusTrap';

const PREVIOUS_LAYER_ICON = '←';
const NEXT_LAYER_ICON = '→';

interface CutSettingsEditorProps {
  layerId: string;
  onClose: () => void;
  onSwitchLayer?: (newLayerId: string) => void;
}

export function CutSettingsEditor({ layerId, onClose, onSwitchLayer }: CutSettingsEditorProps) {
  const { t } = useTranslation();
  const layers = useProjectStore((s) => s.project?.layers ?? []);
  const layer = layers.find((candidate) => candidate.id === layerId) ?? null;
  const updateLayer = useProjectStore((s) => s.updateLayer);
  const dialogRef = useRef<HTMLDivElement>(null);
  useFocusTrap(dialogRef, layer !== null);

  useEffect(() => {
    const handleKeyDown = (event: KeyboardEvent) => {
      if (event.key === 'Escape') {
        onClose();
      }
    };
    window.addEventListener('keydown', handleKeyDown);
    return () => window.removeEventListener('keydown', handleKeyDown);
  }, [onClose]);

  if (!layer) {
    return null;
  }

  const layerIndex = layers.findIndex((candidate) => candidate.id === layerId);
  const previousLayer = layerIndex > 0 ? layers[layerIndex - 1] : null;
  const nextLayer = layerIndex >= 0 && layerIndex < layers.length - 1 ? layers[layerIndex + 1] : null;

  return createPortal(
    <div
      ref={dialogRef}
      className="fixed inset-0 z-50 flex items-center justify-center bg-black/50"
      data-testid="cut-settings-overlay"
      onClick={(event) => {
        if (event.target === event.currentTarget) {
          onClose();
        }
      }}
    >
      <div className="flex max-h-[85vh] w-[min(760px,92vw)] flex-col rounded-lg border border-bb-border bg-bb-bg shadow-xl">
        <div className="flex items-center gap-2 border-b border-bb-border px-4 py-3">
          <button
            type="button"
            className="rounded border border-bb-border px-2 py-1 text-xs text-bb-text disabled:opacity-40"
            onClick={() => previousLayer && onSwitchLayer?.(previousLayer.id)}
            disabled={!previousLayer}
            aria-label={t('panels.cut_settings.previous_layer')}
          >
            {PREVIOUS_LAYER_ICON}
          </button>
          <div className="min-w-0 flex-1">
            <div className="text-sm font-medium text-bb-text">{t('panels.cut_settings.title')}</div>
          <input
            data-testid="layer-name-input"
            className="mt-1 w-full rounded border border-bb-border bg-bb-input px-2 py-1 text-xs text-bb-text"
            value={layer.name}
            onChange={(event) => void updateLayer(layer.id, { name: event.target.value })}
            />
          </div>
          <button
            type="button"
            className="rounded border border-bb-border px-2 py-1 text-xs text-bb-text disabled:opacity-40"
            onClick={() => nextLayer && onSwitchLayer?.(nextLayer.id)}
            disabled={!nextLayer}
            aria-label={t('panels.cut_settings.next_layer')}
          >
            {NEXT_LAYER_ICON}
          </button>
          <button
            type="button"
            className="rounded border border-bb-border px-2 py-1 text-xs text-bb-text"
            onClick={onClose}
            data-testid="close-btn"
          >
            {t('common.close')}
          </button>
        </div>
        <div className="overflow-y-auto p-4">
          <SubLayerStack layerId={layer.id} />
        </div>
      </div>
    </div>,
    document.body,
  );
}
