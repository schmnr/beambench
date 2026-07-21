import { useEffect, useMemo, useState, type ReactNode } from 'react';
import { useTranslation } from 'react-i18next';
import { open, save } from '@tauri-apps/plugin-dialog';
import { FilePlus, Import, Trash2 } from 'lucide-react';

import { useArtLibraryStore } from '../../stores/artLibraryStore';
import { useProjectStore } from '../../stores/projectStore';
import { useAppStore } from '../../stores/appStore';
import {
  lengthUnitLabel,
  mmToDisplay,
  roundDisplayLength,
  type DisplayUnit,
} from '../../utils/lengthUnits';
import { ContextMenu } from '../shared/ContextMenu';
import type { ContextMenuEntry } from '../shared/ContextMenu';
import { PanelResizer } from '../layout/PanelResizer';
import type { ArtLibraryItem, ArtLibrarySelectionSnapshot } from '../../types/artLibrary';
import { ART_LIBRARY_DRAG_MIME, encodeArtLibraryDragData } from '../shared/artLibraryDragData';

const inputClass =
  'px-2 py-1 rounded border border-bb-border bg-bb-surface text-xs text-bb-text placeholder:text-bb-text-dim focus:outline-none focus:border-bb-accent';
const detailMutedClass = 'text-[11px] text-bb-text-dim';
const sectionHeaderClass = 'text-xs font-medium text-bb-accent uppercase tracking-wider';
const ICON_SIZE_STORAGE_KEY = 'beam-bench.art-library.icon-size';
const SIDEBAR_WIDTH_STORAGE_KEY = 'beam-bench.art-library.sidebar-width';
const FOOTER_HEIGHT_STORAGE_KEY = 'beam-bench.art-library.footer-height';
const ACTION_BAR_HEIGHT_STORAGE_KEY = 'beam-bench.art-library.action-bar-height';
const DEFAULT_ICON_SIZE = 128;
const MIN_ICON_SIZE = 96;
const MAX_ICON_SIZE = 160;
const DEFAULT_SIDEBAR_WIDTH = 170;
const MIN_SIDEBAR_WIDTH = 120;
const MAX_SIDEBAR_WIDTH = 340;
const DEFAULT_FOOTER_HEIGHT = 34;
const MIN_FOOTER_HEIGHT = 28;
const MAX_FOOTER_HEIGHT = 160;
const DEFAULT_ACTION_BAR_HEIGHT = 120;
const MIN_ACTION_BAR_HEIGHT = 80;
const MAX_ACTION_BAR_HEIGHT = 260;

function getStorage(): Pick<Storage, 'getItem' | 'setItem'> | null {
  if (typeof window === 'undefined') return null;
  const storage = window.localStorage as Partial<Storage> | undefined;
  if (!storage || typeof storage.getItem !== 'function' || typeof storage.setItem !== 'function') {
    return null;
  }
  return storage as Pick<Storage, 'getItem' | 'setItem'>;
}

function fileStem(path: string): string {
  return path.split('/').pop()?.replace(/\.[^.]+$/, '') || 'Library';
}

function clampIconSize(value: number): number {
  return Math.max(MIN_ICON_SIZE, Math.min(MAX_ICON_SIZE, Math.round(value)));
}

function readStoredIconSize(): number {
  const storage = getStorage();
  if (!storage) return DEFAULT_ICON_SIZE;
  const raw = storage.getItem(ICON_SIZE_STORAGE_KEY);
  const parsed = raw ? Number(raw) : NaN;
  return Number.isFinite(parsed) ? clampIconSize(parsed) : DEFAULT_ICON_SIZE;
}

function readStoredSize(key: string, fallback: number, min: number, max: number): number {
  const storage = getStorage();
  if (!storage) return fallback;
  const raw = storage.getItem(key);
  const parsed = raw ? Number(raw) : NaN;
  if (!Number.isFinite(parsed)) return fallback;
  return Math.max(min, Math.min(max, Math.round(parsed)));
}

function decodeBase64Json<T>(data: string): T | null {
  try {
    return JSON.parse(new TextDecoder().decode(Uint8Array.from(atob(data), (c) => c.charCodeAt(0)))) as T;
  } catch {
    return null;
  }
}

function formatLength(mm: number, unit: DisplayUnit): string {
  return `${roundDisplayLength(mmToDisplay(mm, unit), unit)} ${lengthUnitLabel(unit)}`;
}

function deriveItemSizeLabel(item: ArtLibraryItem, unit: DisplayUnit): string | null {
  if (item.kind !== 'selection_snapshot') return null;
  const snapshot = decodeBase64Json<ArtLibrarySelectionSnapshot>(item.data);
  if (!snapshot || snapshot.objects.length === 0) return null;

  let minX = Number.POSITIVE_INFINITY;
  let minY = Number.POSITIVE_INFINITY;
  let maxX = Number.NEGATIVE_INFINITY;
  let maxY = Number.NEGATIVE_INFINITY;
  for (const object of snapshot.objects) {
    minX = Math.min(minX, object.bounds.min.x);
    minY = Math.min(minY, object.bounds.min.y);
    maxX = Math.max(maxX, object.bounds.max.x);
    maxY = Math.max(maxY, object.bounds.max.y);
  }
  if (![minX, minY, maxX, maxY].every(Number.isFinite)) return null;
  const width = maxX - minX;
  const height = maxY - minY;
  if (width <= 0 || height <= 0) return null;
  return `${formatLength(width, unit)} x ${formatLength(height, unit)}`;
}

function describeItemTypeKey(item: ArtLibraryItem): string {
  if (item.kind === 'selection_snapshot') return 'panels.art_library.graphic';
  if (item.media_type.includes('svg')) return 'panels.art_library.vector_graphic';
  if (item.media_type.startsWith('image/')) return 'panels.art_library.raster_graphic';
  return 'panels.art_library.graphic';
}

type RenameDialogState =
  | { target: 'library'; libraryId: string; value: string }
  | { target: 'item'; libraryId: string; itemId: string; value: string };

type DeleteDialogState =
  | { target: 'library'; libraryId: string; name: string }
  | { target: 'item'; libraryId: string; itemId: string; name: string };

function InlineModal({
  title,
  children,
  onClose,
}: {
  title: string;
  children: ReactNode;
  onClose: () => void;
}) {
  return (
    <div
      role="dialog"
      aria-modal="true"
      aria-label={title}
      className="fixed inset-0 z-[110] flex items-center justify-center bg-black/50"
      onClick={(e) => {
        if (e.target === e.currentTarget) onClose();
      }}
    >
      <div className="w-full max-w-sm rounded border border-bb-border bg-bb-panel p-4 shadow-xl">
        <div className="mb-3 text-xs font-medium uppercase tracking-wider text-bb-accent">{title}</div>
        {children}
      </div>
    </div>
  );
}

function ActionButton({
  children,
  icon,
  disabled,
  title,
  className = '',
  onClick,
  testId,
}: {
  children: ReactNode;
  icon?: ReactNode;
  disabled?: boolean;
  title?: string;
  className?: string;
  onClick: () => void;
  testId?: string;
}) {
  return (
    <button
      type="button"
      title={title}
      data-testid={testId}
      disabled={disabled}
      onClick={onClick}
      className={`inline-flex min-h-9 items-center justify-center gap-2 rounded border border-bb-border bg-bb-surface px-3 text-[11px] font-medium text-bb-text transition hover:bg-bb-hover disabled:cursor-default disabled:opacity-40 disabled:text-bb-text-dim ${className}`}
    >
      {icon ? <span className="shrink-0 text-bb-text-muted [&_svg]:h-4 [&_svg]:w-4">{icon}</span> : null}
      <span>{children}</span>
    </button>
  );
}

export function ArtLibraryPanel() {
  const { t } = useTranslation();
  const libraries = useArtLibraryStore((s) => s.libraries);
  const selectedLibraryId = useArtLibraryStore((s) => s.selectedLibraryId);
  const searchQuery = useArtLibraryStore((s) => s.searchQuery);
  const dragState = useArtLibraryStore((s) => s.dragState);
  const project = useProjectStore((s) => s.project);
  const settings = useAppStore((s) => s.settings);
  const displayUnit: DisplayUnit = settings?.display_unit === 'inches' ? 'inches' : 'mm';
  const loadLibraries = useArtLibraryStore((s) => s.loadLibraries);
  const createLibrary = useArtLibraryStore((s) => s.createLibrary);
  const loadLibrary = useArtLibraryStore((s) => s.loadLibrary);
  const unloadLibrary = useArtLibraryStore((s) => s.unloadLibrary);
  const saveLibraryAs = useArtLibraryStore((s) => s.saveLibraryAs);
  const renameLibrary = useArtLibraryStore((s) => s.renameLibrary);
  const deleteLibrary = useArtLibraryStore((s) => s.deleteLibrary);
  const addFileItems = useArtLibraryStore((s) => s.addFileItems);
  const addSelectionItem = useArtLibraryStore((s) => s.addSelectionItem);
  const renameItem = useArtLibraryStore((s) => s.renameItem);
  const removeItem = useArtLibraryStore((s) => s.removeItem);
  const insertToProject = useArtLibraryStore((s) => s.insertToProject);
  const moveItem = useArtLibraryStore((s) => s.moveItem);
  const setSelectedLibrary = useArtLibraryStore((s) => s.setSelectedLibrary);
  const setSearchQuery = useArtLibraryStore((s) => s.setSearchQuery);
  const setDragState = useArtLibraryStore((s) => s.setDragState);

  const [selectedItemId, setSelectedItemId] = useState<string | null>(null);
  const [contextMenu, setContextMenu] = useState<{
    x: number;
    y: number;
    items: ContextMenuEntry[];
  } | null>(null);
  const [renameDialog, setRenameDialog] = useState<RenameDialogState | null>(null);
  const [deleteDialog, setDeleteDialog] = useState<DeleteDialogState | null>(null);
  const [iconSize, setIconSize] = useState<number>(readStoredIconSize);
  const [sidebarWidth, setSidebarWidth] = useState<number>(() =>
    readStoredSize(SIDEBAR_WIDTH_STORAGE_KEY, DEFAULT_SIDEBAR_WIDTH, MIN_SIDEBAR_WIDTH, MAX_SIDEBAR_WIDTH),
  );
  const [footerHeight, setFooterHeight] = useState<number>(() =>
    readStoredSize(FOOTER_HEIGHT_STORAGE_KEY, DEFAULT_FOOTER_HEIGHT, MIN_FOOTER_HEIGHT, MAX_FOOTER_HEIGHT),
  );
  const [actionBarHeight, setActionBarHeight] = useState<number>(() =>
    readStoredSize(ACTION_BAR_HEIGHT_STORAGE_KEY, DEFAULT_ACTION_BAR_HEIGHT, MIN_ACTION_BAR_HEIGHT, MAX_ACTION_BAR_HEIGHT),
  );

  useEffect(() => {
    void loadLibraries();
  }, [loadLibraries]);

  useEffect(() => {
    const storage = getStorage();
    if (!storage) return;
    storage.setItem(ICON_SIZE_STORAGE_KEY, String(iconSize));
  }, [iconSize]);

  useEffect(() => {
    const storage = getStorage();
    if (!storage) return;
    storage.setItem(SIDEBAR_WIDTH_STORAGE_KEY, String(sidebarWidth));
  }, [sidebarWidth]);

  useEffect(() => {
    const storage = getStorage();
    if (!storage) return;
    storage.setItem(FOOTER_HEIGHT_STORAGE_KEY, String(footerHeight));
  }, [footerHeight]);

  useEffect(() => {
    const storage = getStorage();
    if (!storage) return;
    storage.setItem(ACTION_BAR_HEIGHT_STORAGE_KEY, String(actionBarHeight));
  }, [actionBarHeight]);

  const handleSidebarResize = (delta: number) => {
    setSidebarWidth((w) => Math.max(MIN_SIDEBAR_WIDTH, Math.min(MAX_SIDEBAR_WIDTH, w + delta)));
  };

  const handleFooterResize = (delta: number) => {
    setFooterHeight((h) => Math.max(MIN_FOOTER_HEIGHT, Math.min(MAX_FOOTER_HEIGHT, h + delta)));
  };

  const handleActionBarResize = (delta: number) => {
    setActionBarHeight((h) => Math.max(MIN_ACTION_BAR_HEIGHT, Math.min(MAX_ACTION_BAR_HEIGHT, h + delta)));
  };

  const currentLibrary = libraries.find((library) => library.library_id === selectedLibraryId) ?? null;

  const filteredItems = useMemo(() => {
    if (!currentLibrary) return [];
    if (!searchQuery) return currentLibrary.items;
    const q = searchQuery.toLowerCase();
    return currentLibrary.items.filter(
      (item) =>
        item.name.toLowerCase().includes(q)
        || item.tags.some((tag) => tag.toLowerCase().includes(q))
        || item.category.toLowerCase().includes(q),
    );
  }, [currentLibrary, searchQuery]);

  useEffect(() => {
    if (selectedItemId && !currentLibrary?.items.some((item) => item.id === selectedItemId)) {
      setSelectedItemId(null);
    }
  }, [currentLibrary, selectedItemId]);

  const selectedItem = currentLibrary?.items.find((item) => item.id === selectedItemId) ?? null;
  const selectedItemSize = selectedItem ? deriveItemSizeLabel(selectedItem, displayUnit) : null;
  const canInsertIntoProject = Boolean(project);

  async function handleNewLibrary() {
    const path = await save({
      title: t('panels.art_library.dialog_new'),
      defaultPath: 'Untitled.bbart',
      filters: [{ name: t('panels.art_library.filter_library'), extensions: ['bbart'] }],
    });
    if (!path || Array.isArray(path)) return;
    const name = fileStem(path);
    await createLibrary(path, name);
  }

  async function handleLoadLibrary() {
    const path = await open({
      title: t('panels.art_library.dialog_load'),
      multiple: false,
      filters: [{ name: t('panels.art_library.filter_library'), extensions: ['bbart'] }],
    });
    if (!path || Array.isArray(path)) return;
    await loadLibrary(path);
  }

  async function handleSaveAs(library = currentLibrary) {
    if (!library) return;
    const path = await save({
      title: t('panels.art_library.dialog_save_as'),
      defaultPath: library.path,
      filters: [{ name: t('panels.art_library.filter_library'), extensions: ['bbart'] }],
    });
    if (!path || Array.isArray(path)) return;
    await saveLibraryAs(library.library_id, path);
  }

  async function handleAddFile() {
    if (!currentLibrary) return;
    const paths = await open({
      title: t('panels.art_library.dialog_add_items'),
      multiple: true,
      filters: [
        { name: t('panels.art_library.filter_artwork'), extensions: ['svg', 'png', 'jpg', 'jpeg', 'gif', 'bmp', 'webp', 'tif', 'tiff', 'tga', 'dxf', 'pdf', 'ai', 'eps'] },
      ],
    });
    if (!paths || paths.length === 0) return;
    await addFileItems(
      currentLibrary.library_id,
      paths.map((path) => ({ filePath: path, name: fileStem(path) })),
      'General',
      [],
    );
  }

  async function handleAddSelection() {
    if (!currentLibrary) return;
    await addSelectionItem(currentLibrary.library_id, 'Selection', 'General', []);
  }

  function buildEmptyMenu(): ContextMenuEntry[] {
    return [
      { id: 'art-add-file', label: t('context_menu.import'), onClick: () => void handleAddFile() },
      { id: 'art-add-selection', label: t('context_menu.add_selection_to_library'), onClick: () => void handleAddSelection() },
      ...(currentLibrary
        ? [
            { type: 'separator' as const },
            { id: 'library-save-as', label: t('context_menu.save_as'), onClick: () => void handleSaveAs(currentLibrary) },
          ]
        : []),
    ];
  }

  function buildItemMenu(itemId: string, itemName: string): ContextMenuEntry[] {
    return [
      {
        id: 'art-insert',
        label: t('context_menu.insert_into_project'),
        disabled: !canInsertIntoProject,
        onClick: () => {
          if (!currentLibrary || !canInsertIntoProject) return;
          void insertToProject(currentLibrary.library_id, itemId);
        },
      },
      { id: 'art-add-selection', label: t('context_menu.add_selection_to_library'), onClick: () => void handleAddSelection() },
      {
        id: 'art-rename',
        label: t('common.rename'),
        onClick: () => currentLibrary && setRenameDialog({
          target: 'item',
          libraryId: currentLibrary.library_id,
          itemId,
          value: itemName,
        }),
      },
      { type: 'separator' as const },
      {
        id: 'art-delete',
        label: t('context_menu.delete'),
        disabled: !!currentLibrary?.save_error,
        onClick: () => currentLibrary && setDeleteDialog({
          target: 'item',
          libraryId: currentLibrary.library_id,
          itemId,
          name: itemName,
        }),
      },
    ];
  }

  async function handleRenameSubmit() {
    if (!renameDialog) return;
    const next = renameDialog.value.trim();
    if (!next) return;
    if (renameDialog.target === 'library') {
      await renameLibrary(renameDialog.libraryId, next);
    } else {
      await renameItem(renameDialog.libraryId, renameDialog.itemId, next);
    }
    setRenameDialog(null);
  }

  async function handleDeleteConfirm() {
    if (!deleteDialog) return;
    if (deleteDialog.target === 'library') {
      await deleteLibrary(deleteDialog.libraryId);
    } else {
      await removeItem(deleteDialog.libraryId, deleteDialog.itemId);
    }
    setDeleteDialog(null);
  }

  const browserGridStyle = useMemo(
    () => ({
      gridTemplateColumns: `repeat(auto-fill, minmax(${Math.max(108, iconSize + 10)}px, 1fr))`,
    }),
    [iconSize],
  );

  return (
    <div className="h-full min-h-0 overflow-hidden px-2 py-2 text-xs text-bb-text">
      <div className="flex h-full min-h-0 flex-col">
        <div className="flex min-h-0 flex-1">
          <div
            className="flex min-h-0 flex-col rounded border border-bb-border bg-bb-surface"
            style={{ width: sidebarWidth, flex: '0 0 auto' }}
          >
            <div className="min-h-0 flex-1 overflow-y-auto py-1" data-testid="art-library-list">
              {libraries.length === 0 ? (
                <div className="px-3 py-3 text-sm text-bb-text-dim">{t('panels.art_library.no_libraries')}</div>
              ) : (
                libraries.map((library) => {
                  const isActive = library.library_id === selectedLibraryId;
                  const isDragTarget = dragState?.targetLibraryId === library.library_id;
                  return (
                    <button
                      type="button"
                      key={library.library_id}
                      title={library.path ?? library.name}
                      className={`flex w-full flex-col items-start border-l-2 px-3 py-2 text-left transition ${isActive ? 'border-bb-accent bg-bb-accent/10 text-bb-text' : 'border-transparent hover:bg-bb-hover'} ${isDragTarget ? 'ring-1 ring-bb-accent ring-inset' : ''}`}
                      onClick={() => setSelectedLibrary(library.library_id)}
                      onContextMenu={(e) => {
                        e.preventDefault();
                        e.stopPropagation();
                        setSelectedLibrary(library.library_id);
                        setContextMenu({
                          x: e.clientX,
                          y: e.clientY,
                          items: [
                            {
                              id: 'library-rename',
                              label: t('context_menu.rename_library'),
                              onClick: () => setRenameDialog({
                                target: 'library',
                                libraryId: library.library_id,
                                value: library.name,
                              }),
                            },
                            {
                              id: 'library-save-as',
                              label: t('context_menu.save_as'),
                              onClick: () => void handleSaveAs(library),
                            },
                            { type: 'separator' as const },
                            {
                              id: 'library-delete',
                              label: t('context_menu.delete'),
                              disabled: !!library.save_error,
                              onClick: () => setDeleteDialog({
                                target: 'library',
                                libraryId: library.library_id,
                                name: library.name,
                              }),
                            },
                          ],
                        });
                      }}
                      onDragOver={(e) => {
                        const liveDragState = useArtLibraryStore.getState().dragState;
                        if (!liveDragState) return;
                        const dropAllowed = liveDragState.sourceLibraryId !== library.library_id;
                        e.preventDefault();
                        e.dataTransfer.dropEffect = dropAllowed ? (e.shiftKey ? 'move' : 'copy') : 'none';
                        setDragState({
                          ...liveDragState,
                          targetLibraryId: library.library_id,
                          dropAllowed,
                          dropEffect: e.shiftKey ? 'move' : 'copy',
                        });
                      }}
                      onDrop={(e) => {
                        const liveDragState = useArtLibraryStore.getState().dragState;
                        if (!liveDragState || liveDragState.sourceLibraryId === library.library_id) return;
                        e.preventDefault();
                        void moveItem(
                          liveDragState.sourceLibraryId,
                          liveDragState.itemId,
                          library.library_id,
                          e.shiftKey,
                        );
                        setDragState(null);
                      }}
                    >
                      <span
                        className="line-clamp-2 text-[12px] font-semibold leading-5"
                        style={{
                          display: '-webkit-box',
                          WebkitLineClamp: 2,
                          WebkitBoxOrient: 'vertical',
                          overflow: 'hidden',
                        }}
                      >
                        {library.name}
                      </span>
                    </button>
                  );
                })
              )}
            </div>
          </div>

          <div className="mx-1.5 flex items-stretch">
            <PanelResizer direction="left" onResize={handleSidebarResize} />
          </div>

          <div className="flex min-h-0 flex-1 flex-col gap-2 overflow-hidden">
            <div className="flex flex-wrap items-center gap-2">
              <input
                type="text"
                className={`min-w-[140px] flex-[1_1_180px] ${inputClass}`}
                placeholder={t('panels.art_library.search')}
                value={searchQuery}
                onChange={(e) => setSearchQuery(e.target.value)}
              />
              <div className="flex min-w-[150px] flex-[1_1_220px] items-center justify-end gap-2">
                <label className="shrink-0 text-[11px] font-semibold text-bb-text">{t('panels.art_library.icon_size')}</label>
                <input
                  data-testid="art-library-icon-size"
                  type="range"
                  min={MIN_ICON_SIZE}
                  max={MAX_ICON_SIZE}
                  step={8}
                  value={iconSize}
                  onChange={(e) => setIconSize(clampIconSize(Number(e.target.value)))}
                  className="h-2 min-w-0 flex-1 accent-bb-accent"
                />
                <div
                  data-testid="art-library-icon-size-readout"
                  className="min-w-[68px] text-right text-[11px] font-semibold text-bb-text"
                >
                  {iconSize} x {iconSize}
                </div>
              </div>
            </div>

            {currentLibrary?.save_error ? (
              <div className="rounded border border-bb-error-border bg-bb-error-bg px-2 py-1.5 text-[11px] font-medium text-bb-error-fg">
                {t('panels.art_library.save_error', { detail: currentLibrary.save_error })}
              </div>
            ) : null}

            <div
              className="min-h-0 flex-1 overflow-hidden rounded border border-bb-border bg-bb-bg p-2"
              onContextMenu={(e) => {
                e.preventDefault();
                setContextMenu({ x: e.clientX, y: e.clientY, items: buildEmptyMenu() });
              }}
            >
              {!currentLibrary ? (
                <div className="flex h-full items-center justify-center text-sm text-bb-text-dim">
                  {t('panels.art_library.empty_hint')}
                </div>
              ) : filteredItems.length === 0 ? (
                <div className="flex h-full items-center justify-center text-sm text-bb-text-dim">
                  {currentLibrary.items.length === 0 ? t('panels.art_library.no_items') : t('panels.art_library.no_matches')}
                </div>
              ) : (
                <div
                  className="grid h-full content-start gap-x-4 gap-y-4 overflow-y-auto"
                  style={browserGridStyle}
                  data-testid="art-library-browser-grid"
                >
                  {filteredItems.map((item) => {
                    const isSelected = selectedItemId === item.id;
                    const thumbBoxSize = iconSize;
                    return (
                      <button
                        type="button"
                        key={item.id}
                        draggable
                        data-testid={`art-item-${item.id}`}
                        className={`group flex flex-col items-center gap-2 rounded px-1 py-2 text-center transition ${isSelected ? 'bg-bb-accent/15 outline outline-1 outline-bb-accent/60' : 'hover:bg-bb-hover'}`}
                        onClick={() => setSelectedItemId(item.id)}
                        onDoubleClick={() => {
                          if (!canInsertIntoProject) return;
                          void insertToProject(currentLibrary.library_id, item.id);
                        }}
                        onContextMenu={(e) => {
                          e.preventDefault();
                          e.stopPropagation();
                          setSelectedItemId(item.id);
                          setContextMenu({
                            x: e.clientX,
                            y: e.clientY,
                            items: buildItemMenu(item.id, item.name),
                          });
                        }}
                        onDragStart={(e) => {
                          e.dataTransfer.effectAllowed = 'copyMove';
                          e.dataTransfer.setData(
                            ART_LIBRARY_DRAG_MIME,
                            encodeArtLibraryDragData({
                              sourceLibraryId: currentLibrary.library_id,
                              itemId: item.id,
                            }),
                          );
                          e.dataTransfer.setData('text/plain', item.id);
                          setDragState({
                            sourceLibraryId: currentLibrary.library_id,
                            itemId: item.id,
                            dropEffect: 'copy',
                            dropAllowed: true,
                            targetLibraryId: null,
                          });
                        }}
                        onDragEnd={() => setDragState(null)}
                      >
                        <div
                          className="flex items-center justify-center overflow-hidden rounded border border-bb-border bg-white"
                          style={{ width: thumbBoxSize, height: thumbBoxSize }}
                        >
                          {item.thumbnail ? (
                            <img
                              src={`data:image/png;base64,${item.thumbnail}`}
                              alt={item.name}
                              className="h-full w-full object-contain"
                            />
                          ) : (
                            <span className="text-[30px] text-bb-text-dim">
                              {item.kind === 'selection_snapshot' ? '◫' : item.media_type.includes('svg') ? '◇' : '▣'}
                            </span>
                          )}
                        </div>
                        <span
                          className="max-w-full truncate text-[11px] font-medium text-bb-text"
                          title={item.name}
                        >
                          {item.name}
                        </span>
                      </button>
                    );
                  })}
                </div>
              )}
            </div>

            <div className="my-1.5">
              <PanelResizer direction="bottom" onResize={handleFooterResize} />
            </div>

            <div
              className="overflow-y-auto rounded border border-bb-border bg-bb-surface px-2 py-1.5"
              style={{ height: footerHeight, flex: '0 0 auto' }}
            >
              {selectedItem ? (
                <div className="flex flex-wrap items-center gap-x-3 gap-y-1">
                  <span className="text-[11px] font-medium text-bb-text">{selectedItem.name}</span>
                  {selectedItemSize ? <span className={detailMutedClass}>{selectedItemSize}</span> : null}
                  <span className={`${detailMutedClass} uppercase tracking-wider text-bb-accent`}>
                    {t(describeItemTypeKey(selectedItem))}
                  </span>
                </div>
              ) : (
                <div className={detailMutedClass}>{t('panels.art_library.no_graphic_selected')}</div>
              )}
            </div>
          </div>
        </div>

        <div className="my-1.5">
          <PanelResizer direction="bottom" onResize={handleActionBarResize} />
        </div>

        <div
          className="flex flex-wrap items-start gap-x-6 gap-y-2 overflow-y-auto px-1 pb-1 pt-1"
          style={{ height: actionBarHeight, flex: '0 0 auto' }}
        >
          <div className="flex min-w-[168px] flex-col gap-1.5">
            <div className={sectionHeaderClass}>{t('panels.registry.art_library')}</div>
            <div className="grid grid-cols-2 gap-2">
              <ActionButton
                title={t('panels.art_library.new_library')}
                testId="art-library-new"
                icon={<FilePlus />}
                onClick={() => void handleNewLibrary()}
              >
                {t('panels.art_library.new')}
              </ActionButton>
              <ActionButton
                title={t('panels.art_library.load_library')}
                testId="art-library-load"
                onClick={() => void handleLoadLibrary()}
              >
                {t('panels.art_library.load')}
              </ActionButton>
              <ActionButton
                title={t('panels.art_library.unload_library')}
                testId="art-library-unload"
                className="col-span-2 justify-self-center px-6"
                disabled={!currentLibrary || !!currentLibrary.save_error}
                onClick={() => {
                  if (!currentLibrary) return;
                  void unloadLibrary(currentLibrary.library_id);
                }}
              >
                {t('panels.art_library.unload')}
              </ActionButton>
            </div>
          </div>

          <div className="flex min-w-[240px] flex-1 flex-col gap-1.5">
            <div className={sectionHeaderClass}>{t('panels.art_library.graphic')}</div>
            <div className="flex flex-wrap items-start gap-3">
              <div className="grid min-w-[220px] flex-1 grid-rows-2 gap-2">
                <ActionButton
                  testId="art-library-add-to-project"
                  disabled={!currentLibrary || !selectedItem || !canInsertIntoProject}
                  onClick={() => {
                    if (!currentLibrary || !selectedItem || !canInsertIntoProject) return;
                    void insertToProject(currentLibrary.library_id, selectedItem.id);
                  }}
                >
                  {t('panels.art_library.add_to_project')}
                </ActionButton>
                <ActionButton
                  testId="art-library-import-from-project"
                  disabled={!currentLibrary || !!currentLibrary.save_error}
                  onClick={() => void handleAddSelection()}
                >
                  {t('panels.art_library.import_from_project')}
                </ActionButton>
              </div>

              <div className="grid min-w-[135px] grid-rows-2 gap-2">
                <ActionButton
                  testId="art-library-import"
                  icon={<Import />}
                  disabled={!currentLibrary || !!currentLibrary.save_error}
                  onClick={() => void handleAddFile()}
                >
                  {t('context_menu.import')}
                </ActionButton>
                <ActionButton
                  testId="art-library-delete"
                  icon={<Trash2 />}
                  disabled={!currentLibrary || !selectedItem || !!currentLibrary.save_error}
                  onClick={() => selectedItem && setDeleteDialog({
                    target: 'item',
                    libraryId: currentLibrary!.library_id,
                    itemId: selectedItem.id,
                    name: selectedItem.name,
                  })}
                >
                  {t('context_menu.delete')}
                </ActionButton>
              </div>
            </div>
          </div>
        </div>
      </div>

      {contextMenu ? (
        <ContextMenu
          x={contextMenu.x}
          y={contextMenu.y}
          items={contextMenu.items}
          onClose={() => setContextMenu(null)}
        />
      ) : null}

      {renameDialog ? (
        <InlineModal
          title={renameDialog.target === 'library' ? t('context_menu.rename_library') : t('panels.art_library.rename_item')}
          onClose={() => setRenameDialog(null)}
        >
          <div className="flex flex-col gap-2">
            <input
              autoFocus
              type="text"
              className={`w-full ${inputClass}`}
              value={renameDialog.value}
              onChange={(e) => setRenameDialog({ ...renameDialog, value: e.target.value })}
              onKeyDown={(e) => {
                if (e.key === 'Enter') void handleRenameSubmit();
                if (e.key === 'Escape') setRenameDialog(null);
              }}
              data-testid="art-library-rename-input"
            />
            <div className="flex justify-end gap-2">
              <ActionButton className="px-3" onClick={() => setRenameDialog(null)}>
                {t('common.cancel')}
              </ActionButton>
              <ActionButton className="border-bb-accent/60 bg-bb-accent/20 px-3 text-bb-text hover:bg-bb-accent/30" onClick={() => void handleRenameSubmit()}>
                {t('common.save')}
              </ActionButton>
            </div>
          </div>
        </InlineModal>
      ) : null}

      {deleteDialog ? (
        <InlineModal
          title={deleteDialog.target === 'library' ? t('panels.art_library.delete_library') : t('panels.art_library.delete_item')}
          onClose={() => setDeleteDialog(null)}
        >
          <div className="flex flex-col gap-3">
            <p className="text-sm text-bb-text">
              {deleteDialog.target === 'library'
                ? t('panels.art_library.confirm_delete_library', { name: deleteDialog.name })
                : t('panels.art_library.confirm_delete_item', { name: deleteDialog.name })}
            </p>
            <div className="flex justify-end gap-2">
              <ActionButton className="px-3" onClick={() => setDeleteDialog(null)}>
                {t('common.cancel')}
              </ActionButton>
              <ActionButton className="border-bb-error-border bg-bb-error-bg px-3 text-bb-error-fg hover:bg-bb-error-bg/80" onClick={() => void handleDeleteConfirm()}>
                {t('context_menu.delete')}
              </ActionButton>
            </div>
          </div>
        </InlineModal>
      ) : null}
    </div>
  );
}
