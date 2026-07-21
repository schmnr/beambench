import { useEffect, useMemo, useRef, useState } from 'react';
import { wrapBackendError } from '../../i18n/errors';
import { createPortal } from 'react-dom';
import { useTranslation } from 'react-i18next';
import { useProjectStore } from '../../stores/projectStore';
import { useNotificationStore } from '../../stores/notificationStore';
import { vectorService } from '../../services/vectorService';
import type { Bounds, ProjectObject } from '../../types/project';
import type { BooleanAssistantOperation, BooleanAssistantPreview } from '../../types/vector';
import { useFocusTrap } from '../../hooks/useFocusTrap';

interface BooleanAssistantDialogProps {
  objectIds: string[];
  onClose: () => void;
}

const SOURCE_COLORS = ['#94A3B8', '#F59E0B', '#A78BFA', '#34D399'];

function mergeBounds(bounds: Bounds[]): Bounds | null {
  if (bounds.length === 0) return null;
  return bounds.reduce<Bounds>((acc, bounds) => ({
    min: {
      x: Math.min(acc.min.x, bounds.min.x),
      y: Math.min(acc.min.y, bounds.min.y),
    },
    max: {
      x: Math.max(acc.max.x, bounds.max.x),
      y: Math.max(acc.max.y, bounds.max.y),
    },
  }), bounds[0]);
}

function resultPathData(object: ProjectObject | null): string {
  if (object?.data.type !== 'vector_path') return '';
  return object.data.path_data.trim();
}

function previewViewBox(preview: BooleanAssistantPreview | null): string {
  const bounds = preview
    ? mergeBounds([
      ...preview.sources.map((source) => source.bounds),
      preview.result.bounds,
    ])
    : null;
  if (!bounds) return '0 0 100 100';

  const width = Math.max(bounds.max.x - bounds.min.x, 1);
  const height = Math.max(bounds.max.y - bounds.min.y, 1);
  const padding = Math.max(width, height) * 0.08;
  return [
    bounds.min.x - padding,
    bounds.min.y - padding,
    width + padding * 2,
    height + padding * 2,
  ].join(' ');
}

export function BooleanAssistantDialog({ objectIds, onClose }: BooleanAssistantDialogProps) {
  const { t } = useTranslation();
  const projectId = useProjectStore((s) => s.project?.metadata.project_id ?? null);

  const operationOptions: Array<{ value: BooleanAssistantOperation; label: string }> = [
    { value: 'union', label: t('dialog.boolean_assistant.op_union') },
    { value: 'subtract', label: t('dialog.boolean_assistant.op_subtract') },
    { value: 'intersection', label: t('dialog.boolean_assistant.op_intersection') },
    { value: 'weld', label: t('dialog.boolean_assistant.op_weld') },
    { value: 'exclude', label: t('dialog.boolean_assistant.op_exclude') },
  ];
  const booleanPending = useProjectStore((s) => s.booleanPending);
  const [operation, setOperation] = useState<BooleanAssistantOperation>('union');
  const [preview, setPreview] = useState<BooleanAssistantPreview | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const initialProjectIdRef = useRef(projectId);
  const dialogRef = useRef<HTMLDivElement>(null);
  useFocusTrap(dialogRef, true);

  const activeObjectIds = useMemo(
    () => (operation === 'weld' ? objectIds : objectIds.slice(0, 2)),
    [objectIds, operation],
  );
  const resultPath = resultPathData(preview?.result ?? null);
  const canCommit = resultPath.length > 0 && !loading && !error && !booleanPending;

  useEffect(() => {
    const handleKey = (e: KeyboardEvent) => {
      if (e.key === 'Escape') onClose();
    };
    window.addEventListener('keydown', handleKey);
    return () => window.removeEventListener('keydown', handleKey);
  }, [onClose]);

  useEffect(() => {
    if (projectId !== initialProjectIdRef.current) {
      onClose();
    }
  }, [projectId, onClose]);

  useEffect(() => {
    let cancelled = false;
    setLoading(true);
    setError(null);
    setPreview(null);

    vectorService.booleanAssistantPreview(activeObjectIds, operation)
      .then((nextPreview) => {
        if (!cancelled) setPreview(nextPreview);
      })
      .catch((err) => {
        if (!cancelled) setError(wrapBackendError(String(err)));
      })
      .finally(() => {
        if (!cancelled) setLoading(false);
      });

    return () => {
      cancelled = true;
    };
  }, [activeObjectIds, operation]);

  const handleCommit = async () => {
    const currentProject = useProjectStore.getState().project;
    const currentProjectId = currentProject?.metadata.project_id ?? null;
    if (currentProjectId !== initialProjectIdRef.current) {
      useNotificationStore.getState().push(t('dialog.boolean_assistant.error_project_changed'), 'warning');
      onClose();
      return;
    }

    if (!currentProject || activeObjectIds.some((id) => !currentProject.objects.some((object) => object.id === id))) {
      useNotificationStore.getState().push(t('dialog.boolean_assistant.error_objects_unavailable'), 'warning');
      onClose();
      return;
    }

    const store = useProjectStore.getState();
    if (operation === 'union') {
      await store.booleanUnion(activeObjectIds[0], activeObjectIds[1]);
    } else if (operation === 'subtract') {
      await store.booleanSubtract(activeObjectIds[0], activeObjectIds[1]);
    } else if (operation === 'intersection') {
      await store.booleanIntersection(activeObjectIds[0], activeObjectIds[1]);
    } else if (operation === 'exclude') {
      await store.booleanExclude(activeObjectIds[0], activeObjectIds[1]);
    } else {
      await store.booleanWeld(activeObjectIds);
    }
    onClose();
  };

  return createPortal(
    <div
      ref={dialogRef}
      role="dialog"
      aria-modal="true"
      aria-labelledby="boolean-assistant-title"
      className="fixed inset-0 bg-black/50 flex items-center justify-center z-50"
      onClick={(e) => { if (e.target === e.currentTarget) onClose(); }}
    >
      <div className="bg-bb-panel border border-bb-border rounded-lg shadow-xl p-4 w-[520px] max-w-[calc(100vw-2rem)]">
        <div className="flex items-center justify-between gap-3 mb-3">
          <h2 id="boolean-assistant-title" className="text-sm font-semibold text-bb-text">{t('dialog.boolean_assistant.title')}</h2>
          <span className="text-[11px] text-bb-text-muted">{t('dialog.boolean_assistant.shapes_count', { count: activeObjectIds.length })}</span>
        </div>

        <div className="grid grid-cols-5 gap-1 mb-3" role="group" aria-label={t('dialog.boolean_assistant.title')}>
          {operationOptions.map((option) => (
            <button
              key={option.value}
              type="button"
              onClick={() => setOperation(option.value)}
              className={`px-2 py-1.5 text-xs font-medium rounded border ${
                operation === option.value
                  ? 'bg-bb-accent text-bb-on-accent border-bb-accent'
                  : 'bg-bb-bg text-bb-text border-bb-border hover:bg-bb-hover'
              }`}
            >
              {option.label}
            </button>
          ))}
        </div>

        <div className="border border-bb-border bg-bb-bg rounded min-h-[260px] overflow-hidden relative">
          {preview && resultPath && (
            <svg
              className="w-full h-[260px]"
              viewBox={previewViewBox(preview)}
              preserveAspectRatio="xMidYMid meet"
              data-testid="boolean-assistant-preview"
            >
              {preview.sources.map((source, index) => (
                <path
                  key={source.id}
                  d={source.pathData}
                  fill="none"
                  stroke={SOURCE_COLORS[index % SOURCE_COLORS.length]}
                  strokeWidth={1.2}
                  strokeDasharray="3 2"
                  vectorEffect="non-scaling-stroke"
                  opacity={0.85}
                />
              ))}
              <path
                d={resultPath}
                fill="rgba(34, 192, 238, 0.30)"
                stroke="rgb(34, 192, 238)"
                strokeWidth={1.6}
                vectorEffect="non-scaling-stroke"
              />
            </svg>
          )}
          {loading && (
            <div className="absolute inset-0 flex items-center justify-center text-xs text-bb-text-muted">
              {t('dialog.boolean_assistant.building_preview')}
            </div>
          )}
          {!loading && error && (
            <div className="absolute inset-0 flex items-center justify-center px-4 text-center text-xs text-bb-error-fg">
              {error}
            </div>
          )}
          {!loading && !error && preview && !resultPath && (
            <div className="absolute inset-0 flex items-center justify-center text-xs text-bb-text-muted">
              {t('dialog.boolean_assistant.no_geometry')}
            </div>
          )}
        </div>

        <div className="mt-3 flex items-center justify-between gap-3">
          <div className="min-w-0 text-[11px] text-bb-text-muted truncate">
            {operation === 'subtract'
              ? t('dialog.boolean_assistant.subtract_template', {
                  first: preview?.sources[0]?.name ?? t('dialog.boolean_assistant.first_shape'),
                  second: preview?.sources[1]?.name ?? t('dialog.boolean_assistant.second_shape'),
                })
              : operationOptions.find((option) => option.value === operation)?.label}
          </div>
          <div className="flex justify-end gap-2">
            <button onClick={onClose} className="px-3 py-1 text-xs font-medium rounded bg-bb-bg hover:bg-bb-hover text-bb-text">
              {t('common.cancel')}
            </button>
            <button
              data-testid="boolean-assistant-apply"
              disabled={!canCommit}
              onClick={() => void handleCommit()}
              className={`px-3 py-1 text-xs font-medium rounded ${
                canCommit
                  ? 'bg-bb-accent hover:bg-bb-accent-hover text-bb-on-accent'
                  : 'bg-bb-surface text-bb-text-muted cursor-not-allowed'
              }`}
            >
              {t('common.apply')}
            </button>
          </div>
        </div>
      </div>
    </div>,
    document.body,
  );
}
