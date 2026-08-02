import { useEffect, useState } from 'react';
import { createPortal } from 'react-dom';
import { useTranslation } from 'react-i18next';
import { useAppStore } from '../../stores/appStore';
import { appService } from '../../services/appService';
import { useNotificationStore } from '../../stores/notificationStore';
import type { BuildInfo } from '../../types/commands';
import { DIALOG_TONE, DialogButton, DialogFooter, DialogSectionHeader } from '../shared/DialogPrimitives';
import { MovableResizableDialogFrame } from '../shared/MovableResizableDialogFrame';

const FACEBOOK_GROUP_URL = 'https://www.facebook.com/groups/beambench';
const CRAFTGINEER_URL = 'https://craftgineer.com';
const SOURCE_CODE_URL = 'https://github.com/schmnr/beambench';
const GPL_LICENSE_URL = 'https://www.gnu.org/licenses/gpl-3.0.html';
const POTRACE_URL = 'https://potrace.sourceforge.net/';
const APP_BRAND_NAME = 'Beam Bench';

function formatBuildDate(raw: string, unknownLabel: string, locale: string): string {
  if (!raw || raw === 'unknown-time') return unknownLabel;
  const parsed = new Date(raw);
  if (Number.isNaN(parsed.getTime())) return raw;
  return parsed.toLocaleDateString(locale, {
    year: 'numeric',
    month: 'short',
    day: 'numeric',
  });
}

function shortSha(sha: string, unknownLabel: string): string {
  if (!sha || sha.startsWith('unknown')) return unknownLabel;
  return sha.slice(0, 7);
}

function buildVersionInfoText(
  version: string,
  build: BuildInfo | null,
  unknownLabel: string,
  locale: string,
): string {
  const lines = [
    `Beam Bench ${version} (beta)`,
  ];
  if (build) {
    lines.push(`build ${shortSha(build.git_sha, unknownLabel)} · ${formatBuildDate(build.build_timestamp, unknownLabel, locale)}`);
    lines.push(`${build.target_triple} · rustc ${build.rustc_version}`);
  }
  return lines.join('\n');
}

export function AboutDialog({ onClose }: { onClose: () => void }) {
  const { t, i18n } = useTranslation();
  const status = useAppStore((s) => s.status);
  const [build, setBuild] = useState<BuildInfo | null>(null);
  const pushNotification = useNotificationStore((s) => s.push);
  const version = status?.version ?? t('common.unknown');
  const unknownLabel = t('common.unknown');

  useEffect(() => {
    let cancelled = false;
    appService
      .getBuildInfo()
      .then((info) => {
        if (!cancelled) setBuild(info);
      })
      .catch((err) => {
        console.warn('[Beam Bench] Failed to load build info', err);
      });
    return () => {
      cancelled = true;
    };
  }, []);

  const handleCopyVersion = async () => {
    const text = buildVersionInfoText(version, build, unknownLabel, i18n.language);
    try {
      await navigator.clipboard.writeText(text);
      pushNotification(t('dialog.about.copied_toast'), 'success');
    } catch {
      pushNotification(t('dialog.about.copy_failed_toast'), 'error');
    }
  };

  const openExternal = (url: string) => {
    void appService.openExternalUrl(url).catch((err) => {
      console.warn('[Beam Bench] Failed to open external URL', url, err);
      pushNotification(t('dialog.about.open_link_failed_toast'), 'error');
    });
  };

  return createPortal(
    <MovableResizableDialogFrame
      title={APP_BRAND_NAME}
      titleId="about-dialog-title"
      testId="about-dialog"
      initialWidth={480}
      initialHeight={680}
      minWidth={420}
      minHeight={520}
      onRequestClose={onClose}
      closeOnBackdropClick
      zIndexClassName="z-[9000]"
      footer={<DialogFooter><DialogButton tone={DIALOG_TONE.primary} onClick={onClose}>{t('common.close')}</DialogButton></DialogFooter>}
    >
      <div className="min-h-0 flex-1 overflow-y-auto bg-bb-bg/20">
        {/* Identity */}
        <div className="flex flex-col items-center pt-7 pb-5 px-6">
          <img
            src="/icon.png"
            alt=""
            className="w-16 h-16 rounded-xl mb-3 select-none"
            draggable={false}
          />
          <div className="flex items-center gap-2">
            <h2 className="text-xl font-semibold text-bb-text leading-none">
              {APP_BRAND_NAME}
            </h2>
            <span className="text-[10px] font-bold tracking-wider text-bb-accent border border-bb-accent rounded px-1.5 py-0.5 uppercase">
              {t('dialog.about.beta_badge')}
            </span>
          </div>
          <p className="text-sm text-bb-text-muted mt-1.5">
            {t('dialog.about.tagline')}
          </p>
        </div>

        {/* Version & build */}
        <div className="border-t border-bb-border">
          <DialogSectionHeader title={t('dialog.about.version_section')} />
          <div className="px-6 py-4">
          <div className="font-mono text-sm text-bb-text">
            {version}
          </div>
          <div className="font-mono text-xs text-bb-text-muted mt-1">
            {t('dialog.about.build_line', {
              sha: shortSha(build?.git_sha ?? '', unknownLabel),
              date: formatBuildDate(build?.build_timestamp ?? '', unknownLabel, i18n.language),
            })}
          </div>
          <div className="font-mono text-xs text-bb-text-dim mt-0.5">
            {t('dialog.about.target_line', {
              triple: build?.target_triple ?? 'unknown-target',
              rustc: build?.rustc_version ?? unknownLabel,
            })}
          </div>
          <p className="text-xs text-bb-text-muted mt-3 italic">
            {t('dialog.about.beta_warning')}
          </p>
          <button
            onClick={handleCopyVersion}
            className="mt-3 h-8 rounded-lg border border-bb-border bg-bb-surface px-3 text-xs text-bb-text transition-colors hover:border-bb-accent/30 hover:bg-bb-hover"
          >
            {t('dialog.about.copy_version')}
          </button>
          </div>
        </div>

        {/* Get involved */}
        <div className="border-t border-bb-border">
          <DialogSectionHeader title={t('dialog.about.get_involved_section')} />
          <div className="px-6 py-4">

          <button
            onClick={() => openExternal(FACEBOOK_GROUP_URL)}
            className="group mb-3 w-full rounded-lg border border-bb-border bg-bb-surface px-3 py-2.5 text-left transition-colors hover:border-bb-accent/30 hover:bg-bb-hover"
          >
            <div className="flex items-center justify-between">
              <div>
                <div className="text-sm text-bb-text font-medium">
                  {t('dialog.about.facebook_title')}
                </div>
                <div className="text-xs text-bb-text-muted mt-0.5">
                  {t('dialog.about.facebook_subtitle')}
                </div>
              </div>
              <span className="text-bb-text-muted group-hover:text-bb-accent transition-colors ml-3">
                ↗
              </span>
            </div>
          </button>

          <button
            onClick={() => openExternal(CRAFTGINEER_URL)}
            className="group w-full rounded-lg border border-bb-border bg-bb-surface px-3 py-2.5 text-left transition-colors hover:border-bb-accent/30 hover:bg-bb-hover"
          >
            <div className="flex items-center justify-between">
              <div>
                <div className="text-sm text-bb-text font-medium">
                  {t('dialog.about.craftgineer_title')}
                </div>
                <div className="text-xs text-bb-text-muted mt-0.5">
                  {t('dialog.about.craftgineer_subtitle')}
                </div>
              </div>
              <span className="text-bb-text-muted group-hover:text-bb-accent transition-colors ml-3">
                ↗
              </span>
            </div>
          </button>
          </div>
        </div>

        {/* Free-software and third-party notices */}
        <div className="border-t border-bb-border">
          <DialogSectionHeader title={t('dialog.about.free_software_section')} />
          <div className="px-6 py-4">
          <p className="text-xs text-bb-text-muted leading-relaxed">
            {t('dialog.about.gpl_summary')}
          </p>
          <p className="text-xs text-bb-text-muted leading-relaxed mt-2">
            {t('dialog.about.potrace_notice')}
          </p>
          <div className="flex flex-wrap gap-2 mt-3">
            <button
              onClick={() => openExternal(SOURCE_CODE_URL)}
              className="h-8 rounded-lg border border-bb-border bg-bb-surface px-3 text-xs text-bb-text transition-colors hover:border-bb-accent/30 hover:bg-bb-hover"
            >
              {t('dialog.about.source_code')} ↗
            </button>
            <button
              onClick={() => openExternal(GPL_LICENSE_URL)}
              className="h-8 rounded-lg border border-bb-border bg-bb-surface px-3 text-xs text-bb-text transition-colors hover:border-bb-accent/30 hover:bg-bb-hover"
            >
              {t('dialog.about.gpl_license')} ↗
            </button>
            <button
              onClick={() => openExternal(POTRACE_URL)}
              className="h-8 rounded-lg border border-bb-border bg-bb-surface px-3 text-xs text-bb-text transition-colors hover:border-bb-accent/30 hover:bg-bb-hover"
            >
              {t('dialog.about.potrace_link')} ↗
            </button>
          </div>
          </div>
        </div>
      </div>
    </MovableResizableDialogFrame>,
    document.body,
  );
}
