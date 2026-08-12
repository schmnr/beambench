import { useEffect, useRef } from 'react';
import { createPortal } from 'react-dom';
import { useTranslation } from 'react-i18next';
import { X } from 'lucide-react';
import { appService } from '../../services/appService';
import { useWelcomeStore } from '../../stores/welcomeStore';
import { useNotificationStore } from '../../stores/notificationStore';
import craftgineerMark from '../../assets/welcome/craftgineer-mark.svg';
import photoConverterPoster from '../../assets/welcome/craftgineer-photo-converter-poster.png';
import photoConverterPreview from '../../assets/welcome/craftgineer-photo-converter-preview.webp';
import clocklabPoster from '../../assets/welcome/craftgineer-clocklab-poster.png';
import clocklabPreview from '../../assets/welcome/craftgineer-clocklab-preview.webp';
import vectorStudioPoster from '../../assets/welcome/craftgineer-vector-studio-poster.png';
import vectorStudioPreview from '../../assets/welcome/craftgineer-vector-studio-preview.webp';
import printCutCarveMark from '../../assets/welcome/printcutcarve-mark.svg';
import printCutCarvePirate from '../../assets/welcome/printcutcarve-pirate.jpg';
import printCutCarveWedding from '../../assets/welcome/printcutcarve-wedding.jpg';
import printCutCarveBoards from '../../assets/welcome/printcutcarve-boards.jpg';
import './WelcomeDialog.css';

const CRAFTGINEER_URL = 'https://craftgineer.com';
const PRINTCUTCARVE_URL = 'https://printcutcarve.com';

interface PromoPreviewTileProps {
  name: string;
  outcome: string;
  preview: string;
  poster: string;
}

function PromoPreviewTile({ name, outcome, preview, poster }: PromoPreviewTileProps) {
  return (
    <figure className="welcome-cg-tool">
      <div className="welcome-cg-media">
        <picture>
          <source media="(prefers-reduced-motion: reduce)" srcSet={poster} />
          <img
            src={preview}
            alt=""
            aria-hidden="true"
            onError={(event) => {
              const image = event.currentTarget;
              image.onerror = null;
              image.src = poster;
            }}
          />
        </picture>
      </div>
      <figcaption className="welcome-cg-caption">
        <span className="welcome-cg-name">{name}</span>
        <span className="welcome-cg-outcome">{outcome}</span>
      </figcaption>
    </figure>
  );
}

/**
 * Startup promotion. Animated WebP previews preserve motion without invoking
 * Linux WebKitGTK's GStreamer/H.264 playback path. Every visual is imported
 * through Vite so the packaged app receives a verified, hashed asset URL.
 */
export function WelcomeDialog() {
  const { t } = useTranslation();
  const overlayRef = useRef<HTMLDivElement>(null);
  const closeDialog = useWelcomeStore((s) => s.closeDialog);
  const pushNotification = useNotificationStore((s) => s.push);

  useEffect(() => {
    overlayRef.current?.focus();
    const closeOnEscape = (event: KeyboardEvent) => {
      if (event.key === 'Escape') closeDialog();
    };
    window.addEventListener('keydown', closeOnEscape);
    return () => window.removeEventListener('keydown', closeOnEscape);
  }, [closeDialog]);

  const openExternal = (url: string) => {
    // Keep the panel open so the user can visit both products.
    void appService.openExternalUrl(url).catch((err) => {
      console.warn('[Beam Bench] Failed to open external URL', url, err);
      pushNotification(t('dialog.welcome.open_link_failed'), 'error');
    });
  };

  return createPortal(
    <div
      ref={overlayRef}
      role="dialog"
      aria-modal="true"
      aria-labelledby="welcome-dialog-title"
      tabIndex={-1}
      className="welcome-dialog-backdrop fixed inset-0 z-[9000] flex items-center justify-center"
      onClick={(e) => {
        if (e.target === e.currentTarget) closeDialog();
      }}
    >
      <div className="welcome-dialog-shell">
        <button
          onClick={closeDialog}
          aria-label={t('dialog.welcome.close_aria')}
          className="welcome-dialog-close"
        >
          <X size={18} />
        </button>

        <div className="welcome-dialog-content">
          <header className="welcome-dialog-heading">
            <h2 id="welcome-dialog-title">{t('dialog.welcome.title')}</h2>
            <p>{t('dialog.welcome.subtitle')}</p>
          </header>

          <div className="welcome-brand-grid">
            <article className="welcome-brand-panel welcome-cg-panel">
              <div className="welcome-brand-hero welcome-cg-hero">
                <img src={craftgineerMark} alt="" />
                <h3>{t('dialog.welcome.craftgineer_name')}</h3>
              </div>
              <div className="welcome-brand-body">
                <p className="welcome-brand-tagline welcome-cg-tagline">
                  {t('dialog.welcome.craftgineer_tagline')}
                </p>
                <p className="welcome-brand-description welcome-cg-description">
                  {t('dialog.welcome.craftgineer_description')}
                </p>

                <div className="welcome-cg-showcase">
                  <div className="welcome-cg-showcase-heading">
                    <span>{t('dialog.welcome.craftgineer_showcase')}</span>
                    <span>{t('dialog.welcome.craftgineer_watch')}</span>
                  </div>
                  <div className="welcome-cg-reel">
                    <PromoPreviewTile
                      name={t('dialog.welcome.photo_converter_name')}
                      outcome={t('dialog.welcome.photo_converter_outcome')}
                      preview={photoConverterPreview}
                      poster={photoConverterPoster}
                    />
                    <PromoPreviewTile
                      name={t('dialog.welcome.clocklab_name')}
                      outcome={t('dialog.welcome.clocklab_outcome')}
                      preview={clocklabPreview}
                      poster={clocklabPoster}
                    />
                    <PromoPreviewTile
                      name={t('dialog.welcome.vector_studio_name')}
                      outcome={t('dialog.welcome.vector_studio_outcome')}
                      preview={vectorStudioPreview}
                      poster={vectorStudioPoster}
                    />
                  </div>
                </div>

                <button
                  onClick={() => openExternal(CRAFTGINEER_URL)}
                  className="welcome-brand-cta welcome-cg-cta"
                >
                  {t('dialog.welcome.craftgineer_visit')}
                </button>
              </div>
            </article>

            <article className="welcome-brand-panel welcome-pcc-panel">
              <div className="welcome-brand-hero welcome-pcc-hero">
                <img src={printCutCarveMark} alt="" />
                <h3>{t('dialog.welcome.printcutcarve_name')}</h3>
              </div>
              <div className="welcome-brand-body">
                <p className="welcome-brand-tagline welcome-pcc-tagline">
                  {t('dialog.welcome.printcutcarve_tagline')}
                </p>
                <p className="welcome-brand-description welcome-pcc-description">
                  {t('dialog.welcome.printcutcarve_description')}
                </p>

                <div className="welcome-pcc-gallery" aria-hidden="true">
                  <img src={printCutCarvePirate} alt="" />
                  <img src={printCutCarveWedding} alt="" />
                  <img src={printCutCarveBoards} alt="" />
                </div>

                <div className="welcome-pcc-offer">
                  <strong>{t('dialog.welcome.printcutcarve_offer')}</strong>
                  <span>{t('dialog.welcome.printcutcarve_offer_detail')}</span>
                </div>

                <button
                  onClick={() => openExternal(PRINTCUTCARVE_URL)}
                  className="welcome-brand-cta welcome-pcc-cta"
                >
                  {t('dialog.welcome.printcutcarve_visit')}
                </button>
              </div>
            </article>
          </div>
        </div>
      </div>
    </div>,
    document.body,
  );
}
