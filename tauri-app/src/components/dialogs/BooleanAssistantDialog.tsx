import { useEffect, useMemo, useRef, useState } from 'react';
import { wrapBackendError } from '../../i18n/errors';
import { createPortal } from 'react-dom';
import { useTranslation } from 'react-i18next';
import { useProjectStore } from '../../stores/projectStore';
import { useNotificationStore } from '../../stores/notificationStore';
import { vectorService } from '../../services/vectorService';
import type { Bounds, ProjectObject } from '../../types/project';
import type { BooleanAssistantOperation, BooleanAssistantPreview } from '../../types/vector';
import { Combine } from 'lucide-react';
import { MovableResizableDialogFrame } from '../shared/MovableResizableDialogFrame';
import { DIALOG_TONE, DialogButton, DialogFooter, DialogSectionHeader } from '../shared/DialogPrimitives';

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

  const activeObjectIds = useMemo(
    () => (operation === 'weld' ? objectIds : objectIds.slice(0, 2)),
    [objectIds, operation],
  );
  const resultPath = resultPathData(preview?.result ?? null);
  const canCommit = resultPath.length > 0 && !loading && !error && !booleanPending;

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
    <MovableResizableDialogFrame
      title={t('dialog.boolean_assistant.title')}
      titleId="boolean-assistant-title"
      testId="boolean-assistant-dialog"
      initialWidth={580}
      initialHeight={520}
      minWidth={500}
      minHeight={440}
      onRequestClose={onClose}
      closeOnBackdropClick
      headerActions={(
        <span className="rounded-full border border-bb-border bg-bb-bg/70 px-2 py-0.5 text-[11px] text-bb-text-muted">
          {t('dialog.boolean_assistant.shapes_count', { count: activeObjectIds.length })}
        </span>
      )}
      footer={(
        <DialogFooter
          leading={(
            <div className="max-w-[320px] truncate text-[11px] text-bb-text-muted">
              {operation === 'subtract'
                ? t('dialog.boolean_assistant.subtract_template', {
                    first: preview?.sources[0]?.name ?? t('dialog.boolean_assistant.first_shape'),
                    second: preview?.sources[1]?.name ?? t('dialog.boolean_assistant.second_shape'),
                  })
                : operationOptions.find((option) => option.value === operation)?.label}
            </div>
          )}
        >
          <DialogButton tone={DIALOG_TONE.quiet} onClick={onClose}>{t('common.cancel')}</DialogButton>
          <DialogButton tone={DIALOG_TONE.primary} data-testid="boolean-assistant-apply" disabled={!canCommit} onClick={() => void handleCommit()}>
            {t('common.apply')}
          </DialogButton>
        </DialogFooter>
      )}
    >
      <div className="flex min-h-0 flex-1 flex-col bg-bb-bg/20">
        <DialogSectionHeader icon={<Combine size={14} />} title={t('dialog.boolean_assistant.title')} />
        <div className="grid grid-cols-5 gap-1 border-b border-bb-border bg-bb-panel p-3" role="group" aria-label={t('dialog.boolean_assistant.title')}>
          {operationOptions.map((option) => (
            <button
              key={option.value}
              type="button"
              onClick={() => setOperation(option.value)}
              className={`h-8 rounded-lg border px-2 text-xs font-medium transition-colors ${
                operation === option.value
                  ? 'border-bb-accent/45 bg-bb-accent/10 text-bb-text'
                  : 'border-bb-border bg-bb-bg text-bb-text-muted hover:border-bb-accent/30 hover:bg-bb-hover hover:text-bb-text'
              }`}
            >
              {option.label}
            </button>
          ))}
        </div>

        <div className="relative m-3 min-h-0 flex-1 overflow-hidden rounded-xl border border-bb-accent/30 bg-bb-bg shadow-inner">
          {preview && resultPath && (
            <svg
              className="h-full w-full"
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

      </div>
    </MovableResizableDialogFrame>,
    document.body,
  );
}
