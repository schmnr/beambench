import { useState, useEffect, useRef } from 'react';
import { createPortal } from 'react-dom';
import { useTranslation } from 'react-i18next';
import { useProjectStore } from '../../stores/projectStore';

interface NotesDialogProps {
  onClose: () => void;
}

export function NotesDialog({ onClose }: NotesDialogProps) {
  const { t } = useTranslation();
  const projectId = useProjectStore((s) => s.project?.metadata.project_id ?? null);
  const existingNotes = useProjectStore((s) => s.project?.notes ?? '');
  const [notes, setNotes] = useState(existingNotes);
  const initialProjectIdRef = useRef(projectId);
  const overlayRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    overlayRef.current?.focus();
  }, [onClose]);

  useEffect(() => {
    if (projectId !== initialProjectIdRef.current) {
      onClose();
    }
  }, [projectId, onClose]);

  const handleSave = async () => {
    const currentProjectId = useProjectStore.getState().project?.metadata.project_id ?? null;
    if (currentProjectId !== initialProjectIdRef.current) {
      onClose();
      return;
    }
    // skip the backend round-trip if notes haven't changed, so Save
    // on an unchanged form doesn't create an empty undo step or dirty the project.
    const currentNotes = useProjectStore.getState().project?.notes ?? '';
    if (notes === currentNotes) {
      onClose();
      return;
    }
    const saved = await useProjectStore.getState().updateProjectNotes(notes);
    if (saved) {
      onClose();
    }
  };

  return createPortal(
    <div
      ref={overlayRef}
      role="dialog"
      aria-modal="true"
      aria-labelledby="dialog-title"
      tabIndex={-1}
      className="fixed inset-0 bg-black/50 flex items-center justify-center z-50"
      onKeyDown={(e) => {
        if (e.key === 'Escape') onClose();
      }}
      onClick={(e) => { if (e.target === e.currentTarget) onClose(); }}
    >
      <div className="bg-bb-panel border border-bb-border rounded-lg shadow-xl p-4 min-w-[400px]">
        <h2 id="dialog-title" className="text-sm font-semibold text-bb-text mb-3">{t('dialog.notes.title')}</h2>
        <textarea
          data-testid="notes-textarea"
          value={notes}
          onChange={(e) => setNotes(e.target.value)}
          className="w-full h-40 px-2 py-1.5 bg-bb-bg border border-bb-border rounded text-xs text-bb-text resize-y focus:outline-none focus:border-bb-accent"
          placeholder={t('dialog.notes.placeholder')}
        />
        <div className="flex justify-end gap-2 mt-4">
          <button onClick={onClose} className="px-3 py-1 text-xs font-medium rounded bg-bb-bg hover:bg-bb-hover text-bb-text">{t('common.cancel')}</button>
          <button data-testid="notes-save" onClick={() => void handleSave()} className="px-3 py-1 text-xs font-medium rounded bg-bb-accent hover:bg-bb-accent-hover text-bb-on-accent">{t('common.save')}</button>
        </div>
      </div>
    </div>,
    document.body
  );
}
