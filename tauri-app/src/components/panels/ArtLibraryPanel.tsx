import { useEffect, useMemo, useState, type ReactNode } from 'react';
import { useTranslation } from 'react-i18next';
import { open, save } from '@tauri-apps/plugin-dialog';
import { CopyPlus, FolderOpen, Import, Plus, X } from 'lucide-react';

import { useArtLibraryStore } from '../../stores/artLibraryStore';
import { useProjectStore } from '../../stores/projectStore';
import { ContextMenu } from '../shared/ContextMenu';
import type { ContextMenuEntry } from '../shared/ContextMenu';
import { IconButton } from '../shared/IconButton';
import { ART_LIBRARY_DRAG_MIME, encodeArtLibraryDragData } from '../shared/artLibraryDragData';
import { rangeTrackBackground } from '../shared/RangeInput';

const inputClass =
  'px-2 py-1 rounded border border-bb-border bg-bb-surface text-xs text-bb-text placeholder:text-bb-text-dim focus:outline-none focus:border-bb-accent';
const sectionHeaderClass = 'text-xs font-medium text-bb-accent uppercase tracking-wider';
const ICON_SIZE_STORAGE_KEY = 'beam-bench.art-library.icon-size';
const DEFAULT_ICON_SIZE = 128;
const MIN_ICON_SIZE = 96;
const MAX_ICON_SIZE = 160;

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

  const [contextMenu, setContextMenu] = useState<{
    x: number;
    y: number;
    items: ContextMenuEntry[];
  } | null>(null);
  const [renameDialog, setRenameDialog] = useState<RenameDialogState | null>(null);
  const [deleteDialog, setDeleteDialog] = useState<DeleteDialogState | null>(null);
  const [iconSize, setIconSize] = useState<number>(readStoredIconSize);

  useEffect(() => {
    void loadLibraries();
  }, [loadLibraries]);

  useEffect(() => {
    const storage = getStorage();
    if (!storage) return;
    storage.setItem(ICON_SIZE_STORAGE_KEY, String(iconSize));
  }, [iconSize]);

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
        <div className="flex min-h-0 flex-1 flex-col gap-1.5">
          <div
            className="flex min-h-0 flex-col rounded border border-bb-border bg-bb-surface"
            style={{ height: 132, flex: '0 0 auto' }}
          >
            <div className="flex h-9 shrink-0 items-center gap-1 border-b border-bb-border px-2">
              <div className={`${sectionHeaderClass} min-w-0 flex-1 truncate`}>
                {t('panels.registry.art_library')}
              </div>
              <IconButton
                size="xs"
                icon={<Plus size={17} />}
                label={t('panels.art_library.new_library')}
                onClick={() => void handleNewLibrary()}
              />
              <IconButton
                size="xs"
                icon={<FolderOpen size={17} />}
                label={t('panels.art_library.load_library')}
                onClick={() => void handleLoadLibrary()}
              />
            </div>
            <div className="min-h-0 flex-1 overflow-y-auto py-1" data-testid="art-library-list">
              {libraries.length === 0 ? (
                <div className="px-3 py-3 text-sm text-bb-text-dim">{t('panels.art_library.no_libraries')}</div>
              ) : (
                libraries.map((library) => {
                  const isActive = library.library_id === selectedLibraryId;
                  const isDragTarget = dragState?.targetLibraryId === library.library_id;
                  return (
                    <div
                      key={library.library_id}
                      className={`group flex w-full items-center border-l-2 pr-1 transition ${isActive ? 'border-bb-accent bg-bb-accent/10 text-bb-text' : 'border-transparent hover:bg-bb-hover'} ${isDragTarget ? 'ring-1 ring-bb-accent ring-inset' : ''}`}
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
                      <button
                        type="button"
                        title={library.path ?? library.name}
                        className="min-w-0 flex-1 px-3 py-1.5 text-left"
                        onClick={() => setSelectedLibrary(library.library_id)}
                      >
                        <span className="block truncate text-[12px] font-semibold leading-5">
                          {library.name}
                        </span>
                      </button>
                      <span className={isActive ? 'opacity-80' : 'opacity-0 group-hover:opacity-70 group-focus-within:opacity-70'}>
                        <IconButton
                          size="xs"
                          icon={<X size={15} />}
                          label={`${t('panels.art_library.unload_library')}: ${library.name}`}
                          disabled={!!library.save_error}
                          onClick={() => void unloadLibrary(library.library_id)}
                        />
                      </span>
                    </div>
                  );
                })
              )}
            </div>
          </div>

          <div className="flex min-h-0 flex-1 flex-col gap-2 overflow-hidden">
            <div className="flex h-8 shrink-0 items-center gap-1 px-1">
              <div className={`${sectionHeaderClass} min-w-0 flex-1 truncate`}>
                {t('panels.art_library.graphic')}
              </div>
              <IconButton
                size="xs"
                icon={<Import size={16} />}
                label={t('context_menu.import')}
                disabled={!currentLibrary || !!currentLibrary.save_error}
                onClick={() => void handleAddFile()}
              />
              <IconButton
                size="xs"
                icon={<CopyPlus size={16} />}
                label={t('panels.art_library.import_from_project')}
                disabled={!currentLibrary || !!currentLibrary.save_error}
                onClick={() => void handleAddSelection()}
              />
            </div>
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
                  className="bb-range min-w-0 flex-1"
                  style={{ background: rangeTrackBackground(iconSize, MIN_ICON_SIZE, MAX_ICON_SIZE) }}
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
                    const thumbBoxSize = iconSize;
                    return (
                      <div
                        key={item.id}
                        draggable
                        role="listitem"
                        aria-label={item.name}
                        data-testid={`art-item-${item.id}`}
                        className="group relative flex flex-col items-center gap-2 rounded px-1 py-2 text-center transition hover:bg-bb-hover"
                        onContextMenu={(e) => {
                          e.preventDefault();
                          e.stopPropagation();
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
                        <button
                          type="button"
                          draggable={false}
                          data-testid={`art-item-delete-${item.id}`}
                          aria-label={`${t('panels.art_library.delete_item')}: ${item.name}`}
                          title={t('panels.art_library.delete_item')}
                          className="absolute right-2 top-2 z-10 flex h-6 w-6 items-center justify-center rounded-md border border-bb-border bg-bb-panel/90 text-bb-text-muted opacity-70 shadow-sm transition hover:bg-bb-error-bg hover:text-bb-error-fg hover:opacity-100 focus:opacity-100"
                          onMouseDown={(e) => e.stopPropagation()}
                          onDragStart={(e) => e.preventDefault()}
                          onContextMenu={(e) => e.stopPropagation()}
                          onClick={(e) => {
                            e.stopPropagation();
                            setDeleteDialog({
                              target: 'item',
                              libraryId: currentLibrary.library_id,
                              itemId: item.id,
                              name: item.name,
                            });
                          }}
                        >
                          <X size={14} />
                        </button>
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
                      </div>
                    );
                  })}
                </div>
              )}
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
