import { ChevronRight, X } from 'lucide-react';
import { useTranslation } from 'react-i18next';
import type { ProjectObject } from '../../types/project';

interface SelectionIsolationBreadcrumbProps {
  path: string[];
  objects: ProjectObject[];
  onNavigate: (depth: number) => void;
  onExit: () => void;
}

export function SelectionIsolationBreadcrumb({ path, objects, onNavigate, onExit }: SelectionIsolationBreadcrumbProps) {
  const { t } = useTranslation();
  if (path.length === 0) return null;
  const objectsById = new Map(objects.map((object) => [object.id, object]));

  return (
    <div className="absolute left-8 top-8 z-40 flex max-w-[calc(100%-4rem)] items-center gap-1 rounded-lg border border-bb-accent/30 bg-bb-panel/95 px-2 py-1.5 text-xs text-bb-text-muted shadow-lg backdrop-blur-md">
      <button type="button" className="rounded px-1.5 py-1 text-bb-accent hover:bg-bb-accent/10" onClick={() => onNavigate(0)}>
        {t('selection.canvas')}
      </button>
      {path.map((id, index) => (
        <span key={id} className="flex min-w-0 items-center gap-1">
          <ChevronRight size={12} className="shrink-0 text-bb-text-dim" />
          <button
            type="button"
            className={`max-w-40 truncate rounded px-1.5 py-1 hover:bg-bb-hover ${index === path.length - 1 ? 'text-bb-text' : ''}`}
            onClick={() => onNavigate(index + 1)}
          >
            {objectsById.get(id)?.name || t('selection.group')}
          </button>
        </span>
      ))}
      <button type="button" className="ml-1 rounded p-1 text-bb-text-dim hover:bg-bb-hover hover:text-bb-text" onClick={onExit} aria-label={t('selection.exit_isolation')}>
        <X size={13} />
      </button>
    </div>
  );
}
