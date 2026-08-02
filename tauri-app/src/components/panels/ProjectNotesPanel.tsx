import { useEffect, useRef, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { useProjectStore } from '../../stores/projectStore';

export function ProjectNotesPanel() {
  const { t } = useTranslation();
  const projectId = useProjectStore((state) => state.project?.metadata.project_id ?? null);
  const existingNotes = useProjectStore((state) => state.project?.notes ?? '');
  const [draft, setDraft] = useState(existingNotes);
  const [saving, setSaving] = useState(false);
  const sourceRef = useRef({ projectId, notes: existingNotes });

  useEffect(() => {
    const previous = sourceRef.current;
    if (projectId !== previous.projectId || draft === previous.notes) {
      setDraft(existingNotes);
    }
    sourceRef.current = { projectId, notes: existingNotes };
  }, [draft, existingNotes, projectId]);

  const saveNotes = async () => {
    if (!projectId || saving || draft === existingNotes) return;
    const currentProjectId = useProjectStore.getState().project?.metadata.project_id ?? null;
    if (currentProjectId !== projectId) return;

    setSaving(true);
    try {
      await useProjectStore.getState().updateProjectNotes(draft);
    } finally {
      setSaving(false);
    }
  };

  if (!projectId) {
    return (
      <div className="px-3 py-3 text-xs italic text-bb-text-dim">
        {t('panels.empty.no_project')}
      </div>
    );
  }

  const dirty = draft !== existingNotes;

  return (
    <div className="flex h-full min-h-0 flex-col gap-2 bg-bb-panel p-3">
      <textarea
        data-testid="notes-textarea"
        aria-label={t('dialog.notes.title')}
        value={draft}
        onChange={(event) => setDraft(event.target.value)}
        onKeyDown={(event) => {
          if ((event.ctrlKey || event.metaKey) && event.key.toLowerCase() === 's') {
            event.preventDefault();
            void saveNotes();
          }
        }}
        className="min-h-[88px] flex-1 resize-none rounded border border-bb-border bg-bb-bg px-2.5 py-2 text-xs leading-relaxed text-bb-text outline-none transition-colors focus:border-bb-accent"
        placeholder={t('dialog.notes.placeholder')}
      />
      <div className="flex justify-end">
        <button
          data-testid="notes-save"
          type="button"
          disabled={!dirty || saving}
          onClick={() => void saveNotes()}
          className="rounded bg-bb-accent px-3 py-1.5 text-xs font-medium text-bb-on-accent transition-colors hover:bg-bb-accent-hover disabled:cursor-default disabled:opacity-40"
        >
          {t('common.save')}
        </button>
      </div>
    </div>
  );
}
