import { useEffect, useMemo, useRef, useState } from 'react';
import { wrapBackendError } from '../../i18n/errors';
import { Copy, Eye, EyeOff, FolderOpen, Save, Send, X } from 'lucide-react';
import { useTranslation } from 'react-i18next';
import { feedbackService } from '../../services/feedbackService';
import {
  DIAGNOSTIC_BUNDLE_DISCLOSURE_FIELDS,
  MAX_FEEDBACK_DESCRIPTION_CHARS,
  MAX_FEEDBACK_REPLY_TO_EMAIL_CHARS,
  MAX_FEEDBACK_TITLE_CHARS,
  MAX_PROJECT_ATTACHMENT_RAW_BYTES,
  SUBMIT_FEEDBACK_DISCLOSURE_FIELDS,
  type DiagnosticBundleV1,
  type FeedbackKind,
  type FeedbackSourceContext,
  type SavedReport,
  type SubmitFeedbackResponse,
} from '../../types/feedback';
import { useNotificationStore } from '../../stores/notificationStore';

export interface FeedbackReportDialogProps {
  kind: FeedbackKind;
  title?: string;
  description?: string;
  notes?: string;
  sourceContext?: FeedbackSourceContext;
  onClose: () => void;
}

type SuccessState =
  | { type: 'saved'; report: SavedReport }
  | { type: 'submitted'; report: SubmitFeedbackResponse };
type BusyAction = 'preview' | 'save' | 'submit';

function fieldLabel(field: string): string {
  return field.replace(/_/g, ' ');
}

function byteLabel(bytes: number): string {
  return `${(bytes / (1024 * 1024)).toFixed(1)} MiB`;
}

export function FeedbackReportDialog({
  kind,
  title: initialTitle = '',
  description: initialDescription = '',
  notes: initialNotes = '',
  sourceContext,
  onClose,
	}: FeedbackReportDialogProps) {
  const { t } = useTranslation();
  const [title, setTitle] = useState(initialTitle);
  const [description, setDescription] = useState(initialDescription);
  const [notes, setNotes] = useState(initialNotes);
  const [replyToEmail, setReplyToEmail] = useState('');
  const [includeProjectFile, setIncludeProjectFile] = useState(false);
  const [previewOpen, setPreviewOpen] = useState(false);
  const [preview, setPreview] = useState<DiagnosticBundleV1 | null>(null);
  const [projectSize, setProjectSize] = useState<number | null>(null);
  const [busyAction, setBusyAction] = useState<BusyAction | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [success, setSuccess] = useState<SuccessState | null>(null);
  const previewRequestVersion = useRef(0);

  const requiresDescription = kind === 'bug' || kind === 'crash';
  const busy = busyAction !== null;
  const projectTooLarge = projectSize !== null && projectSize > MAX_PROJECT_ATTACHMENT_RAW_BYTES;

  const input = useMemo(() => ({
    kind,
    title: title.trim() || null,
    description: description.trim() || null,
    notes: notes.trim() || null,
    reply_to_email: replyToEmail.trim() || null,
    include_project_file: includeProjectFile,
    source_context: sourceContext ?? null,
  }), [description, includeProjectFile, kind, notes, replyToEmail, sourceContext, title]);

  useEffect(() => {
    previewRequestVersion.current += 1;
    setPreview(null);
    setPreviewOpen(false);
  }, [input]);

  useEffect(() => {
    let cancelled = false;
    const probe = {
      ...input,
      description: input.description || (requiresDescription ? t('dialog.feedback.draft_report') : null),
      include_project_file: false,
    };
    void feedbackService.previewReport(probe).then((bundle) => {
      if (!cancelled) {
        setProjectSize(bundle.project_metadata?.size_bytes ?? null);
      }
    }).catch(() => {
      if (!cancelled) setProjectSize(null);
    });
    return () => { cancelled = true; };
  }, [input, requiresDescription, t]);

  const validationMessage = (): string | null => {
    if (requiresDescription && description.trim().length === 0) {
      return t('dialog.feedback.validation_description_required');
    }
    if (title.length > MAX_FEEDBACK_TITLE_CHARS) {
      return t('dialog.feedback.validation_title_max', { max: MAX_FEEDBACK_TITLE_CHARS });
    }
    if (description.length > MAX_FEEDBACK_DESCRIPTION_CHARS) {
      return t('dialog.feedback.validation_description_max', { max: MAX_FEEDBACK_DESCRIPTION_CHARS });
    }
    if (notes.length > MAX_FEEDBACK_DESCRIPTION_CHARS) {
      return t('dialog.feedback.validation_note_max', { max: MAX_FEEDBACK_DESCRIPTION_CHARS });
    }
    if (replyToEmail.length > MAX_FEEDBACK_REPLY_TO_EMAIL_CHARS) {
      return t('dialog.feedback.validation_reply_to_max', { max: MAX_FEEDBACK_REPLY_TO_EMAIL_CHARS });
    }
    if (includeProjectFile && projectTooLarge && projectSize !== null) {
      return t('dialog.feedback.validation_project_too_large', {
        size: byteLabel(projectSize),
        limit: byteLabel(MAX_PROJECT_ATTACHMENT_RAW_BYTES),
      });
    }
    return null;
  };

  const ensureValid = (): boolean => {
    const message = validationMessage();
    if (message) {
      setError(message);
      return false;
    }
    return true;
  };

  const loadPreview = async () => {
    if (busy) return;
    setError(null);
    if (!ensureValid()) return;
    setBusyAction('preview');
    const requestVersion = previewRequestVersion.current;
    try {
      const bundle = await feedbackService.previewReport(input);
      if (previewRequestVersion.current === requestVersion) {
        setPreview(bundle);
        setPreviewOpen(true);
      }
    } catch (previewError) {
      setError(wrapBackendError(String(previewError)));
    } finally {
      setBusyAction(null);
    }
  };

  const saveReport = async () => {
    setError(null);
    if (!ensureValid()) return;
    setBusyAction('save');
    try {
      const report = await feedbackService.saveReport(input);
      setSuccess({ type: 'saved', report });
    } catch (saveError) {
      if (!String(saveError).toLowerCase().includes('cancelled')) {
        setError(wrapBackendError(String(saveError)));
      }
    } finally {
      setBusyAction(null);
    }
  };

  const submitReport = async () => {
    setError(null);
    if (!ensureValid()) return;
    setBusyAction('submit');
    try {
      const report = await feedbackService.submitReport(input);
      setSuccess({ type: 'submitted', report });
    } catch (submitError) {
      setError(wrapBackendError(String(submitError)));
    } finally {
      setBusyAction(null);
    }
  };

  if (success?.type === 'saved') {
    return (
      <div className="fixed inset-0 z-[9800] flex items-center justify-center bg-black/35 px-4">
        <div className="w-[520px] max-w-full rounded-lg border border-bb-border bg-bb-panel shadow-2xl">
          <div className="flex items-center justify-between border-b border-bb-border px-5 py-3">
            <h2 className="text-sm font-semibold text-bb-text">{t('dialog.feedback.report_saved')}</h2>
            <button type="button" onClick={onClose} className="rounded p-1 text-bb-text-dim hover:bg-bb-hover">
              <X size={16} />
            </button>
          </div>
          <div className="space-y-4 px-5 py-4 text-sm text-bb-text">
            <div className="rounded border border-bb-border bg-bb-bg p-3 font-mono text-xs break-all">
              {success.report.path}
            </div>
            <div className="flex justify-end gap-2">
              <button
                type="button"
                className="inline-flex items-center gap-1 rounded border border-bb-border px-3 py-1.5 text-xs hover:bg-bb-hover"
                onClick={() => { void feedbackService.revealReport(success.report.path); }}
              >
                <FolderOpen size={14} /> {t('dialog.feedback.reveal')}
              </button>
              <button
                type="button"
                className="rounded bg-bb-accent px-3 py-1.5 text-xs font-medium text-bb-on-accent hover:bg-bb-accent-hover"
                onClick={onClose}
              >
                {t('dialog.feedback.done')}
              </button>
            </div>
          </div>
        </div>
      </div>
    );
  }

  if (success?.type === 'submitted') {
    return (
      <div className="fixed inset-0 z-[9800] flex items-center justify-center bg-black/35 px-4">
        <div className="w-[520px] max-w-full rounded-lg border border-bb-border bg-bb-panel shadow-2xl">
          <div className="flex items-center justify-between border-b border-bb-border px-5 py-3">
            <h2 className="text-sm font-semibold text-bb-text">{t('dialog.feedback.report_submitted')}</h2>
            <button type="button" onClick={onClose} className="rounded p-1 text-bb-text-dim hover:bg-bb-hover">
              <X size={16} />
            </button>
          </div>
          <div className="space-y-4 px-5 py-4 text-sm text-bb-text">
            <p>{t('dialog.feedback.report_id_intro')}</p>
            <div className="rounded border border-bb-border bg-bb-bg p-3 font-mono text-lg font-semibold break-all">
              {success.report.report_id}
            </div>
            <p className="text-xs text-bb-text-dim">
              {t('dialog.feedback.report_id_help')}
            </p>
            <div className="flex justify-end gap-2">
              <button
                type="button"
                className="inline-flex items-center gap-1 rounded border border-bb-border px-3 py-1.5 text-xs hover:bg-bb-hover"
                onClick={() => {
                  void navigator.clipboard?.writeText(success.report.report_id);
                }}
              >
                <Copy size={14} /> {t('dialog.feedback.copy')}
              </button>
              <button
                type="button"
                className="rounded bg-bb-accent px-3 py-1.5 text-xs font-medium text-bb-on-accent hover:bg-bb-accent-hover"
                onClick={onClose}
              >
                {t('dialog.feedback.done')}
              </button>
            </div>
          </div>
        </div>
      </div>
    );
  }

  return (
    <div className="fixed inset-0 z-[9800] flex items-center justify-center bg-black/35 px-4">
      <div className="flex max-h-[90vh] w-[680px] max-w-full flex-col rounded-lg border border-bb-border bg-bb-panel shadow-2xl">
        <div className="flex items-center justify-between border-b border-bb-border px-5 py-3">
          <h2 className="text-sm font-semibold text-bb-text">
            {kind === 'connectivity' ? t('dialog.feedback.title_connectivity') : t('dialog.feedback.title_bug')}
          </h2>
          <button type="button" onClick={onClose} className="rounded p-1 text-bb-text-dim hover:bg-bb-hover">
            <X size={16} />
          </button>
        </div>

        <div className="min-h-0 flex-1 overflow-auto px-5 py-4">
          <div className="grid gap-3">
            <label className="grid gap-1 text-xs text-bb-text">
              <span className="font-medium">{t('dialog.feedback.field_title')}</span>
              <input
                value={title}
                maxLength={MAX_FEEDBACK_TITLE_CHARS}
                onChange={(event) => setTitle(event.target.value)}
                className="rounded border border-bb-border bg-bb-bg px-2 py-1.5 text-sm outline-none focus:ring-1 focus:ring-bb-accent"
              />
            </label>

            <label className="grid gap-1 text-xs text-bb-text">
              <span className="font-medium">{kind === 'connectivity' ? t('dialog.feedback.field_note') : t('dialog.feedback.field_description')}</span>
              <textarea
                value={kind === 'connectivity' ? notes : description}
                maxLength={MAX_FEEDBACK_DESCRIPTION_CHARS}
                required={requiresDescription}
                rows={5}
                onChange={(event) => {
                  if (kind === 'connectivity') setNotes(event.target.value);
                  else setDescription(event.target.value);
                }}
                className="resize-none rounded border border-bb-border bg-bb-bg px-2 py-1.5 text-sm outline-none focus:ring-1 focus:ring-bb-accent"
              />
            </label>

            <label className="grid gap-1 text-xs text-bb-text">
              <span className="font-medium">{t('dialog.feedback.field_reply_to')}</span>
              <input
                value={replyToEmail}
                maxLength={MAX_FEEDBACK_REPLY_TO_EMAIL_CHARS}
                onChange={(event) => setReplyToEmail(event.target.value)}
                className="rounded border border-bb-border bg-bb-bg px-2 py-1.5 text-sm outline-none focus:ring-1 focus:ring-bb-accent"
              />
            </label>

            <label className={`flex items-start gap-2 rounded border border-bb-border bg-bb-bg/50 p-2 text-xs ${projectTooLarge ? 'text-bb-text-dim' : 'text-bb-text'}`}>
              <input
                type="checkbox"
                className="mt-0.5"
                checked={includeProjectFile}
                disabled={projectTooLarge}
                onChange={(event) => setIncludeProjectFile(event.target.checked)}
              />
              <span>
                {t('dialog.feedback.include_project_file')}
                <span className="block text-bb-text-dim">
                  {projectTooLarge && projectSize !== null
                    ? t('dialog.feedback.validation_project_too_large', {
                        size: byteLabel(projectSize),
                        limit: byteLabel(MAX_PROJECT_ATTACHMENT_RAW_BYTES),
                      })
                    : t('dialog.feedback.include_project_file_help')}
                </span>
              </span>
            </label>

            <div className="rounded border border-bb-border">
              <button
                type="button"
                disabled={busy}
                className="flex w-full items-center justify-between px-3 py-2 text-left text-xs font-medium text-bb-text hover:bg-bb-hover disabled:cursor-not-allowed disabled:opacity-50"
                onClick={() => {
                  if (previewOpen) setPreviewOpen(false);
                  else void loadPreview();
                }}
              >
                <span>{t('dialog.feedback.what_gets_sent')}</span>
                {previewOpen ? <EyeOff size={14} /> : <Eye size={14} />}
              </button>
              <div className="border-t border-bb-border px-3 py-2 text-xs text-bb-text-dim">
                <div className="grid grid-cols-2 gap-x-4 gap-y-1">
                  {[...SUBMIT_FEEDBACK_DISCLOSURE_FIELDS, ...DIAGNOSTIC_BUNDLE_DISCLOSURE_FIELDS.map((field) => `bundle.${field}`)]
                    .map((field) => <div key={field}>{fieldLabel(field)}</div>)}
                </div>
                {previewOpen && (
                  <pre className="mt-3 max-h-64 overflow-auto rounded bg-bb-bg p-3 text-[11px] text-bb-text">
                    {preview ? JSON.stringify(preview, null, 2) : t('dialog.feedback.loading')}
                  </pre>
                )}
              </div>
            </div>

          </div>
        </div>

        <div className="border-t border-bb-border px-5 py-3">
          {error && (
            <div role="alert" className="mb-2 rounded border border-bb-error-border bg-bb-error-bg p-2 text-xs text-bb-error-fg">
              {error}
            </div>
          )}
          {busyAction && (
            <div role="status" className="mb-2 rounded border border-bb-border bg-bb-bg/70 p-2 text-xs text-bb-text-dim">
              {busyAction === 'submit'
                ? t('dialog.feedback.submitting_report')
                : busyAction === 'save'
                  ? t('dialog.feedback.saving_report')
                  : t('dialog.feedback.building_preview')}
            </div>
          )}
          <div className="flex justify-end gap-2">
            <button type="button" className="rounded border border-bb-border px-3 py-1.5 text-xs text-bb-text hover:bg-bb-hover" onClick={onClose}>
              {t('common.cancel')}
            </button>
            <button
              type="button"
              disabled={busy}
              className="inline-flex items-center gap-1 rounded border border-bb-border px-3 py-1.5 text-xs text-bb-text hover:bg-bb-hover disabled:cursor-not-allowed disabled:opacity-50"
              onClick={() => { void saveReport().catch((reportError) => useNotificationStore.getState().push(wrapBackendError(String(reportError)), 'error')); }}
            >
              <Save size={14} /> {busyAction === 'save' ? t('dialog.feedback.saving') : t('dialog.feedback.save_report_to_file')}
            </button>
            <button
              type="button"
              disabled={busy}
              className="inline-flex items-center gap-1 rounded bg-bb-accent px-3 py-1.5 text-xs font-medium text-bb-on-accent hover:bg-bb-accent-hover disabled:cursor-not-allowed disabled:opacity-50"
              onClick={() => { void submitReport().catch((reportError) => useNotificationStore.getState().push(wrapBackendError(String(reportError)), 'error')); }}
            >
              <Send size={14} /> {busyAction === 'submit' ? t('dialog.feedback.submitting') : kind === 'connectivity' ? t('dialog.feedback.send_to_beam_bench') : t('dialog.feedback.submit')}
            </button>
          </div>
        </div>
      </div>
    </div>
  );
}
