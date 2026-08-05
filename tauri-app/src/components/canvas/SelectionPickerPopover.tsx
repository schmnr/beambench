import { useEffect, useMemo, useRef, useState } from 'react';
import { Circle, Group, Image, PenTool, Shapes, Square, Type } from 'lucide-react';
import { useTranslation } from 'react-i18next';
import type { ProjectObject, Layer } from '../../types/project';

interface SelectionPickerPopoverProps {
  x: number;
  y: number;
  objectIds: string[];
  objects: ProjectObject[];
  layers: Layer[];
  viewportWidth: number;
  viewportHeight: number;
  onSelect: (objectId: string) => void;
  onClose: () => void;
}

function ObjectIcon({ object }: { object: ProjectObject }) {
  if (object.data.type === 'group') return <Group size={15} />;
  if (object.data.type === 'text') return <Type size={15} />;
  if (object.data.type === 'raster_image') return <Image size={15} />;
  if (object.data.type === 'vector_path') return <PenTool size={15} />;
  if (object.data.type === 'shape') {
    if (object.data.kind === 'ellipse') return <Circle size={15} />;
    if (object.data.kind === 'rectangle') return <Square size={15} />;
    return <Shapes size={15} />;
  }
  return <Shapes size={15} />;
}

export function SelectionPickerPopover({
  x,
  y,
  objectIds,
  objects,
  layers,
  viewportWidth,
  viewportHeight,
  onSelect,
  onClose,
}: SelectionPickerPopoverProps) {
  const { t } = useTranslation();
  const [activeIndex, setActiveIndex] = useState(0);
  const rootRef = useRef<HTMLDivElement>(null);
  const rows = useMemo(() => {
    const objectsById = new Map(objects.map((object) => [object.id, object]));
    const layersById = new Map(layers.map((layer) => [layer.id, layer]));
    return objectIds.flatMap((id) => {
      const object = objectsById.get(id);
      if (!object) return [];
      return [{ object, layer: layersById.get(object.layer_id) }];
    });
  }, [layers, objectIds, objects]);

  useEffect(() => {
    rootRef.current?.focus();
  }, []);

  useEffect(() => {
    const closeOnOutsidePointer = (event: PointerEvent) => {
      if (!rootRef.current?.contains(event.target as Node)) onClose();
    };
    window.addEventListener('pointerdown', closeOnOutsidePointer, true);
    return () => window.removeEventListener('pointerdown', closeOnOutsidePointer, true);
  }, [onClose]);

  if (rows.length < 2) return null;

  return (
    <div
      ref={rootRef}
      role="listbox"
      aria-label={t('selection.overlap_title')}
      tabIndex={-1}
      className="absolute z-50 w-72 max-h-72 overflow-auto rounded-lg border border-bb-accent/35 bg-bb-panel/95 p-1.5 text-bb-text shadow-2xl backdrop-blur-md outline-none"
      style={{ left: Math.min(x + 10, Math.max(8, viewportWidth - 296)), top: Math.min(y + 10, Math.max(8, viewportHeight - 300)) }}
      onKeyDown={(event) => {
        if (event.key === 'ArrowDown') {
          event.preventDefault();
          setActiveIndex((current) => (current + 1) % rows.length);
        } else if (event.key === 'ArrowUp') {
          event.preventDefault();
          setActiveIndex((current) => (current - 1 + rows.length) % rows.length);
        } else if (event.key === 'Enter') {
          event.preventDefault();
          onSelect(rows[activeIndex].object.id);
        } else if (event.key === 'Escape') {
          event.preventDefault();
          onClose();
        }
      }}
    >
      <div className="px-2 pb-1.5 pt-1 text-[10px] font-semibold uppercase tracking-[0.16em] text-bb-accent/80">
        {t('selection.overlap_title')}
      </div>
      {rows.map(({ object, layer }, index) => (
        <button
          key={object.id}
          type="button"
          role="option"
          aria-selected={index === activeIndex}
          className={`flex w-full items-center gap-2 rounded-md px-2 py-2 text-left transition-colors ${
            index === activeIndex ? 'bg-bb-accent/15 text-bb-text' : 'text-bb-text-muted hover:bg-bb-hover'
          }`}
          onMouseEnter={() => setActiveIndex(index)}
          onClick={() => onSelect(object.id)}
        >
          <span className="flex h-7 w-7 shrink-0 items-center justify-center rounded-md bg-bb-bg/50 text-bb-text-muted">
            <ObjectIcon object={object} />
          </span>
          <span className="min-w-0 flex-1">
            <span className="block truncate text-xs font-medium">{object.name || t('selection.object')}</span>
            <span className="flex items-center gap-1.5 truncate text-[10px] text-bb-text-muted">
              <span className="h-2 w-2 shrink-0 rounded-full" style={{ backgroundColor: layer?.color_tag || '#22d3ee' }} />
              {layer?.name || t('selection.layer')}
            </span>
          </span>
          {index === 0 && <span className="text-[9px] uppercase tracking-wide text-bb-text-dim">{t('selection.topmost')}</span>}
        </button>
      ))}
      <div className="mt-1 border-t border-bb-border px-2 pt-1.5 text-[10px] text-bb-text-dim">
        {t('selection.overlap_hint')}
      </div>
    </div>
  );
}
