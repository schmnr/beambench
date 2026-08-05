import { useMemo, useRef, useState } from 'react';
import { useTranslation } from 'react-i18next';
import {
  Barcode,
  ChevronDown,
  ChevronRight,
  Circle,
  Copy,
  Eye,
  EyeOff,
  GripVertical,
  Group,
  Image as ImageIcon,
  Lock,
  PenTool,
  Pentagon,
  Square,
  Star,
  Type,
  Unlock,
} from 'lucide-react';
import { useProjectStore } from '../../stores/projectStore';
import { useUiStore } from '../../stores/uiStore';
import type { Layer, Project, ProjectObject } from '../../types/project';
import { INSPECTOR_CARD_CLASS, INSPECTOR_HELP_TEXT_CLASS } from '../shared/panelAppearance';
import { WIDE_INSPECTOR_CARD_CLASS } from '../shared/panelAppearance';
import { usePanelHost } from '../../panels';
import {
  displayLayerName,
  layerOperation,
  operationDisplayLabel,
} from '../layers/layerNaming';
import { LayerModeIcon } from '../layers/LayerModeIcon';

type DragState =
  | { kind: 'layer'; layerId: string }
  | { kind: 'object'; rootId: string; objectIds: string[] };

type DropState =
  | { kind: 'layer'; layerId: string; edge: 'before' | 'after' | 'inside' }
  | { kind: 'object'; objectId: string; edge: 'before' | 'after' }
  | null;

type RenameState = {
  kind: 'layer' | 'object';
  id: string;
  value: string;
} | null;

const OUTLINER_DATA_TYPE = 'application/x-beambench-outliner';

function childObjectIds(project: Project): Set<string> {
  const ids = new Set<string>();
  for (const object of project.objects) {
    if (object.data.type !== 'group') continue;
    for (const childId of object.data.children) ids.add(childId);
  }
  return ids;
}

function objectAndDescendantIds(project: Project, objectId: string): string[] {
  const ids: string[] = [];
  const seen = new Set<string>();
  const visit = (id: string) => {
    if (seen.has(id)) return;
    const object = project.objects.find((candidate) => candidate.id === id);
    if (!object) return;
    seen.add(id);
    ids.push(id);
    if (object.data.type === 'group') object.data.children.forEach(visit);
  };
  visit(objectId);
  return ids;
}

export function topLevelObjectsForLayer(project: Project, layerId: string): ProjectObject[] {
  const children = childObjectIds(project);
  return project.objects
    .filter((object) => object.layer_id === layerId && !children.has(object.id))
    .sort((a, b) => b.z_index - a.z_index);
}

export function resolveLayerDropIndex(
  layers: Layer[],
  movingLayerId: string,
  targetLayerId: string,
  edge: 'before' | 'after',
): number {
  const remaining = layers.filter((layer) => layer.id !== movingLayerId);
  const targetIndex = remaining.findIndex((layer) => layer.id === targetLayerId);
  if (targetIndex < 0) return layers.findIndex((layer) => layer.id === movingLayerId);
  return targetIndex + (edge === 'after' ? 1 : 0);
}

export function resolveObjectDropBeforeId(
  project: Project,
  movingObjectIds: string[],
  targetObjectId: string,
  edge: 'before' | 'after',
): string | null {
  const target = project.objects.find((object) => object.id === targetObjectId);
  if (!target) return null;
  const moving = new Set(movingObjectIds);
  const remaining = topLevelObjectsForLayer(project, target.layer_id)
    .filter((object) => !moving.has(object.id));
  const targetIndex = remaining.findIndex((object) => object.id === targetObjectId);
  if (targetIndex < 0) return null;
  const nextRoot = remaining[targetIndex + (edge === 'after' ? 1 : 0)];
  if (!nextRoot) return null;
  return objectAndDescendantIds(project, nextRoot.id)
    .map((id) => project.objects.find((object) => object.id === id))
    .filter((object): object is ProjectObject => Boolean(object))
    .sort((a, b) => b.z_index - a.z_index)[0]?.id ?? nextRoot.id;
}

function ObjectTypeIcon({ object }: { object: ProjectObject }) {
  const props = { size: 14, strokeWidth: 1.8 };
  switch (object.data.type) {
    case 'raster_image': return <ImageIcon {...props} />;
    case 'vector_path': return <PenTool {...props} />;
    case 'shape': return object.data.kind === 'ellipse' ? <Circle {...props} /> : <Square {...props} />;
    case 'star': return <Star {...props} />;
    case 'text': return <Type {...props} />;
    case 'polygon': return <Pentagon {...props} />;
    case 'barcode': return <Barcode {...props} />;
    case 'group': return <Group {...props} />;
    case 'virtual_clone': return <Copy {...props} />;
  }
}

function dropEdgeForRow(event: React.DragEvent<HTMLElement>): 'before' | 'after' {
  const bounds = event.currentTarget.getBoundingClientRect();
  return event.clientY < bounds.top + bounds.height / 2 ? 'before' : 'after';
}

export function OutlinerPanel() {
  const { t } = useTranslation();
  const { orientation } = usePanelHost();
  const project = useProjectStore((state) => state.project);
  const selectedLayerId = useProjectStore((state) => state.selectedLayerId);
  const selectedObjectIds = useProjectStore((state) => state.selectedObjectIds);
  const selectLayer = useProjectStore((state) => state.selectLayer);
  const selectObjects = useProjectStore((state) => state.selectObjects);
  const toggleObjectSelection = useProjectStore((state) => state.toggleObjectSelection);
  const updateLayer = useProjectStore((state) => state.updateLayer);
  const updateObject = useProjectStore((state) => state.updateObject);
  const lockObjects = useProjectStore((state) => state.lockObjects);
  const unlockObjects = useProjectStore((state) => state.unlockObjects);
  const reorderLayer = useProjectStore((state) => state.reorderLayer);
  const moveObjectsInOutliner = useProjectStore((state) => state.moveObjectsInOutliner);
  const workspaceMode = useUiStore((state) => state.workspaceMode);
  const [collapsedLayers, setCollapsedLayers] = useState<Set<string>>(() => new Set());
  const [expandedGroups, setExpandedGroups] = useState<Set<string>>(() => new Set());
  const [drag, setDrag] = useState<DragState | null>(null);
  const dragRef = useRef<DragState | null>(null);
  const [drop, setDrop] = useState<DropState>(null);
  const [rename, setRename] = useState<RenameState>(null);
  const editable = workspaceMode === 'design';

  const objectsById = useMemo(
    () => new Map(project?.objects.map((object) => [object.id, object]) ?? []),
    [project?.objects],
  );

  if (!project) {
    return <div className="px-3 py-4 text-center text-xs italic text-bb-text-dim">{t('panels.empty.no_project')}</div>;
  }

  const toggleExpanded = (current: Set<string>, id: string, setter: (next: Set<string>) => void) => {
    const next = new Set(current);
    if (next.has(id)) next.delete(id);
    else next.add(id);
    setter(next);
  };

  const finishDrag = () => {
    dragRef.current = null;
    setDrag(null);
    setDrop(null);
  };

  const commitRename = async () => {
    if (!rename) return;
    const value = rename.value.trim();
    if (!value) {
      setRename(null);
      return;
    }
    const currentName = rename.kind === 'layer'
      ? project.layers.find((layer) => layer.id === rename.id)?.name
      : project.objects.find((object) => object.id === rename.id)?.name;
    if (currentName === value) {
      setRename(null);
      return;
    }
    const updated = rename.kind === 'layer'
      ? await updateLayer(rename.id, { name: value })
      : await updateObject(rename.id, { name: value });
    if (updated !== false) setRename(null);
  };

  const renameInput = (target: Exclude<RenameState, null>) => (
    <input
      autoFocus
      className="h-6 min-w-0 flex-1 rounded border border-bb-accent bg-bb-input px-1.5 text-xs text-bb-text outline-none"
      value={target.value}
      aria-label={`${t('common.rename')}: ${target.value}`}
      data-testid={`outliner-${target.kind}-rename-input`}
      onFocus={(event) => event.currentTarget.select()}
      onChange={(event) => setRename({ ...target, value: event.target.value })}
      onClick={(event) => event.stopPropagation()}
      onDoubleClick={(event) => event.stopPropagation()}
      onBlur={() => { void commitRename(); }}
      onKeyDown={(event) => {
        event.stopPropagation();
        if (event.key === 'Enter') {
          event.preventDefault();
          event.currentTarget.blur();
        } else if (event.key === 'Escape') {
          event.preventDefault();
          setRename(null);
        }
      }}
    />
  );

  const startObjectDrag = (event: React.DragEvent, object: ProjectObject) => {
    if (!editable) return;
    const roots = selectedObjectIds.includes(object.id) && selectedObjectIds.length > 1
      ? selectedObjectIds
      : [object.id];
    event.stopPropagation();
    event.dataTransfer.effectAllowed = 'move';
    event.dataTransfer.setData(OUTLINER_DATA_TYPE, object.id);
    const nextDrag: DragState = { kind: 'object', rootId: object.id, objectIds: roots };
    dragRef.current = nextDrag;
    setDrag(nextDrag);
  };

  const moveObjectBlock = async (
    targetLayerId: string,
    beforeObjectId: string | null,
  ) => {
    const activeDrag = dragRef.current ?? drag;
    if (activeDrag?.kind !== 'object') return;
    await moveObjectsInOutliner(activeDrag.objectIds, targetLayerId, beforeObjectId);
    finishDrag();
  };

  const renderObject = (object: ProjectObject, depth: number, topLevel: boolean): React.ReactNode => {
    const isGroup = object.data.type === 'group';
    const groupChildren = object.data.type === 'group' ? object.data.children : [];
    const expanded = isGroup && expandedGroups.has(object.id);
    const selected = selectedObjectIds.includes(object.id);
    const dropOnObject = drop?.kind === 'object' && drop.objectId === object.id;
    const descendantIds = objectAndDescendantIds(project, object.id);

    return (
      <div key={object.id}>
        <div
          className={`group relative flex h-8 items-center gap-1 pr-1 text-xs outline-none transition-colors focus-visible:ring-1 focus-visible:ring-inset focus-visible:ring-bb-accent ${
            selected
              ? 'bg-bb-accent/15 text-bb-text'
              : 'text-bb-text-muted hover:bg-bb-hover hover:text-bb-text'
          } ${drag?.kind === 'object' && drag.rootId === object.id ? 'opacity-45' : ''}`}
          style={{ paddingLeft: `${8 + depth * 16}px` }}
          role="treeitem"
          aria-selected={selected}
          aria-expanded={isGroup ? expanded : undefined}
          tabIndex={0}
          draggable={editable && topLevel && !(rename?.kind === 'object' && rename.id === object.id)}
          data-testid={`outliner-object-${object.id}`}
          onDragStart={(event) => startObjectDrag(event, object)}
          onDragEnd={finishDrag}
          onDragOver={topLevel ? (event) => {
            const activeDrag = dragRef.current ?? drag;
            if (activeDrag?.kind !== 'object' || activeDrag.objectIds.includes(object.id)) return;
            event.preventDefault();
            event.stopPropagation();
            event.dataTransfer.dropEffect = 'move';
            setDrop({ kind: 'object', objectId: object.id, edge: dropEdgeForRow(event) });
          } : undefined}
          onDrop={topLevel ? (event) => {
            const activeDrag = dragRef.current ?? drag;
            if (activeDrag?.kind !== 'object') return;
            event.preventDefault();
            event.stopPropagation();
            const edge = dropOnObject ? drop.edge : dropEdgeForRow(event);
            const beforeId = resolveObjectDropBeforeId(project, activeDrag.objectIds, object.id, edge);
            void moveObjectBlock(object.layer_id, beforeId);
          } : undefined}
          onClick={(event) => {
            if (!object.visible) return;
            selectLayer(object.layer_id);
            if (event.shiftKey || event.metaKey || event.ctrlKey) toggleObjectSelection(object.id);
            else selectObjects([object.id]);
          }}
          onKeyDown={(event) => {
            if (event.key === 'F2' && editable) {
              event.preventDefault();
              setRename({ kind: 'object', id: object.id, value: object.name });
              return;
            }
            if (event.key === 'Enter' || event.key === ' ') {
              event.preventDefault();
              if (object.visible) {
                selectLayer(object.layer_id);
                selectObjects([object.id]);
              }
            }
          }}
        >
          {dropOnObject && <span className={`absolute left-2 right-2 h-0.5 bg-bb-accent ${drop.edge === 'before' ? 'top-0' : 'bottom-0'}`} />}
          {editable && topLevel ? <GripVertical size={13} className="shrink-0 text-bb-text-dim/60" /> : <span className="w-[13px] shrink-0" />}
          {isGroup ? (
            <button
              type="button"
              className="flex h-6 w-5 shrink-0 items-center justify-center rounded text-bb-text-dim hover:bg-bb-hover hover:text-bb-text"
              aria-label={expanded ? t('panels.outliner.collapse_group') : t('panels.outliner.expand_group')}
              onClick={(event) => {
                event.stopPropagation();
                toggleExpanded(expandedGroups, object.id, setExpandedGroups);
              }}
            >
              {expanded ? <ChevronDown size={13} /> : <ChevronRight size={13} />}
            </button>
          ) : <span className="w-5 shrink-0" />}
          <span className="shrink-0 text-bb-text-dim"><ObjectTypeIcon object={object} /></span>
          {rename?.kind === 'object' && rename.id === object.id
            ? renameInput(rename)
            : (
              <span
                className={`min-w-0 flex-1 truncate ${object.visible ? '' : 'opacity-55'}`}
                title={editable ? `${object.name} · ${t('common.rename')}` : object.name}
                onDoubleClick={(event) => {
                  if (!editable) return;
                  event.stopPropagation();
                  setRename({ kind: 'object', id: object.id, value: object.name });
                }}
              >
                {object.name}
              </span>
            )}
          {editable && (
            <div className={`flex shrink-0 items-center ${selected || !object.visible || object.locked ? 'opacity-100' : 'opacity-0 group-hover:opacity-100 group-focus-within:opacity-100'}`}>
              <button
                type="button"
                className={`flex h-6 w-6 items-center justify-center rounded hover:bg-bb-hover ${object.visible ? 'text-bb-text-dim hover:text-bb-text' : 'text-bb-accent'}`}
                aria-label={object.visible ? t('panels.outliner.hide_object') : t('panels.outliner.show_object')}
                title={object.visible ? t('panels.outliner.hide_object') : t('panels.outliner.show_object')}
                onClick={(event) => {
                  event.stopPropagation();
                  void updateObject(object.id, { visible: !object.visible });
                }}
              >
                {object.visible ? <Eye size={13} /> : <EyeOff size={13} />}
              </button>
              <button
                type="button"
                className={`flex h-6 w-6 items-center justify-center rounded hover:bg-bb-hover ${object.locked ? 'text-bb-accent' : 'text-bb-text-dim hover:text-bb-text'}`}
                aria-label={object.locked ? t('panels.outliner.unlock_object') : t('panels.outliner.lock_object')}
                title={object.locked ? t('panels.outliner.unlock_object') : t('panels.outliner.lock_object')}
                onClick={(event) => {
                  event.stopPropagation();
                  if (object.locked) void unlockObjects(descendantIds);
                  else void lockObjects(descendantIds);
                }}
              >
                {object.locked ? <Lock size={13} /> : <Unlock size={13} />}
              </button>
            </div>
          )}
        </div>
        {isGroup && expanded && groupChildren
          .map((childId) => objectsById.get(childId))
          .filter((child): child is ProjectObject => Boolean(child))
          .sort((a, b) => b.z_index - a.z_index)
          .map((child) => renderObject(child, depth + 1, false))}
      </div>
    );
  };

  return (
    <section
      className={`${orientation === 'wide' ? WIDE_INSPECTOR_CARD_CLASS : INSPECTOR_CARD_CLASS} flex min-h-0 flex-col`}
      data-testid="outliner-panel"
    >
      <div className="flex h-9 items-center border-b border-bb-border bg-gradient-to-r from-bb-accent/10 to-bb-surface/30 px-3">
        <span className="text-[10px] font-semibold uppercase tracking-[0.16em] text-bb-text-dim">
          {t('panels.outliner.summary', { layers: project.layers.length, objects: project.objects.length })}
        </span>
      </div>
      {editable && orientation !== 'wide' && (
        <div className={`border-b border-bb-border px-3 py-2 ${INSPECTOR_HELP_TEXT_CLASS}`} data-testid="outliner-help">
          {t('panels.outliner.help', {
            defaultValue: 'Drag to reorder or move. Double-click to rename. Hidden objects stay here so you can restore them.',
          })}
        </div>
      )}
      <div
        className={orientation === 'wide' ? 'bb-bottom-outliner-tree min-h-0 flex-1' : 'min-h-0 flex-1 overflow-auto py-1'}
        role="tree"
        aria-label={t('panels.registry.outliner')}
      >
        {project.layers.map((layer) => {
          const objects = topLevelObjectsForLayer(project, layer.id);
          const expanded = !collapsedLayers.has(layer.id);
          const active = selectedLayerId === layer.id;
          const dropOnLayer = drop?.kind === 'layer' && drop.layerId === layer.id;
          const operation = layerOperation(layer);
          const visibleName = displayLayerName(layer);
          return (
            <div key={layer.id} data-outliner-layer-group>
              <div
                className={`group relative flex h-9 items-center gap-1 px-1 text-xs outline-none transition-colors focus-visible:ring-1 focus-visible:ring-inset focus-visible:ring-bb-accent ${
                  active ? 'bg-bb-accent/10 text-bb-text' : 'text-bb-text-muted hover:bg-bb-hover hover:text-bb-text'
                } ${drag?.kind === 'layer' && drag.layerId === layer.id ? 'opacity-45' : ''} ${dropOnLayer && drop.edge === 'inside' ? 'bg-bb-accent/15 ring-1 ring-inset ring-bb-accent/60' : ''}`}
                role="treeitem"
                data-testid={`outliner-layer-${layer.id}`}
                aria-expanded={expanded}
                aria-selected={active}
                tabIndex={0}
                draggable={editable && !(rename?.kind === 'layer' && rename.id === layer.id)}
                onDragStart={(event) => {
                  if (!editable) return;
                  event.dataTransfer.effectAllowed = 'move';
                  event.dataTransfer.setData(OUTLINER_DATA_TYPE, layer.id);
                  const nextDrag: DragState = { kind: 'layer', layerId: layer.id };
                  dragRef.current = nextDrag;
                  setDrag(nextDrag);
                }}
                onDragEnd={finishDrag}
                onDragOver={(event) => {
                  const activeDrag = dragRef.current ?? drag;
                  if (!activeDrag) return;
                  event.preventDefault();
                  event.dataTransfer.dropEffect = 'move';
                  if (activeDrag.kind === 'layer') {
                    if (activeDrag.layerId === layer.id) return;
                    setDrop({ kind: 'layer', layerId: layer.id, edge: dropEdgeForRow(event) });
                  } else {
                    setDrop({ kind: 'layer', layerId: layer.id, edge: 'inside' });
                  }
                }}
                onDrop={(event) => {
                  const activeDrag = dragRef.current ?? drag;
                  if (!activeDrag) return;
                  event.preventDefault();
                  if (activeDrag.kind === 'layer') {
                    if (activeDrag.layerId !== layer.id) {
                      const edge = dropOnLayer && drop.edge !== 'inside' ? drop.edge : dropEdgeForRow(event);
                      void reorderLayer(activeDrag.layerId, resolveLayerDropIndex(project.layers, activeDrag.layerId, layer.id, edge));
                    }
                    finishDrag();
                    return;
                  }
                  const movingExpanded = new Set(activeDrag.objectIds.flatMap((id) => objectAndDescendantIds(project, id)));
                  const firstRemaining = objects.find((object) => !movingExpanded.has(object.id));
                  const beforeId = firstRemaining
                    ? objectAndDescendantIds(project, firstRemaining.id)
                      .map((id) => project.objects.find((object) => object.id === id))
                      .filter((object): object is ProjectObject => Boolean(object))
                      .sort((a, b) => b.z_index - a.z_index)[0]?.id ?? firstRemaining.id
                    : null;
                  void moveObjectBlock(layer.id, beforeId);
                }}
                onClick={() => selectLayer(layer.id)}
                onKeyDown={(event) => {
                  if (event.key === 'F2' && editable) {
                    event.preventDefault();
                    setRename({ kind: 'layer', id: layer.id, value: visibleName });
                    return;
                  }
                  if (event.key === 'Enter' || event.key === ' ') {
                    event.preventDefault();
                    selectLayer(layer.id);
                  }
                }}
              >
                {dropOnLayer && drop.edge !== 'inside' && <span className={`absolute left-2 right-2 h-0.5 bg-bb-accent ${drop.edge === 'before' ? 'top-0' : 'bottom-0'}`} />}
                {editable ? <GripVertical size={13} className="shrink-0 text-bb-text-dim/60" /> : <span className="w-[13px] shrink-0" />}
                <button
                  type="button"
                  className="flex h-6 w-5 shrink-0 items-center justify-center rounded text-bb-text-dim hover:bg-bb-hover hover:text-bb-text"
                  aria-label={expanded ? t('panels.outliner.collapse_layer') : t('panels.outliner.expand_layer')}
                  onClick={(event) => {
                    event.stopPropagation();
                    toggleExpanded(collapsedLayers, layer.id, setCollapsedLayers);
                  }}
                >
                  {expanded ? <ChevronDown size={13} /> : <ChevronRight size={13} />}
                </button>
                <span className="h-3 w-3 shrink-0 rounded-sm border border-bb-border" style={{ backgroundColor: layer.color_tag }} />
                <span
                  className="flex shrink-0 items-center text-bb-text-dim"
                  role="img"
                  aria-label={`${t('panels.layers.header.mode')}: ${operationDisplayLabel(operation)}`}
                  title={operationDisplayLabel(operation)}
                >
                  <LayerModeIcon operation={operation} size={14} testId={`outliner-mode-${operation}`} />
                </span>
                {rename?.kind === 'layer' && rename.id === layer.id
                  ? renameInput(rename)
                  : (
                    <span
                      className={`min-w-0 flex-1 truncate font-medium ${layer.visible ? '' : 'opacity-55'}`}
                      title={editable ? `${visibleName} · ${t('common.rename')}` : visibleName}
                      onDoubleClick={(event) => {
                        if (!editable) return;
                        event.stopPropagation();
                        setRename({ kind: 'layer', id: layer.id, value: visibleName });
                      }}
                    >
                      {visibleName}
                    </span>
                  )}
                <span className="shrink-0 rounded bg-bb-bg px-1.5 py-0.5 text-[10px] tabular-nums text-bb-text-dim">{objects.length}</span>
                {editable && (
                  <button
                    type="button"
                    className={`flex h-6 w-6 shrink-0 items-center justify-center rounded hover:bg-bb-hover ${layer.visible ? 'text-bb-text-dim hover:text-bb-text' : 'text-bb-accent'}`}
                    aria-label={layer.visible ? t('panels.outliner.hide_layer') : t('panels.outliner.show_layer')}
                    title={layer.visible ? t('panels.outliner.hide_layer') : t('panels.outliner.show_layer')}
                    onClick={(event) => {
                      event.stopPropagation();
                      void updateLayer(layer.id, { visible: !layer.visible });
                    }}
                  >
                    {layer.visible ? <Eye size={14} /> : <EyeOff size={14} />}
                  </button>
                )}
              </div>
              {expanded && (
                <div role="group">
                  {objects.length > 0
                    ? objects.map((object) => renderObject(object, 1, true))
                    : <div className="h-7 pl-14 text-[11px] italic leading-7 text-bb-text-dim">{t('panels.outliner.empty_layer')}</div>}
                </div>
              )}
            </div>
          );
        })}
      </div>
    </section>
  );
}
