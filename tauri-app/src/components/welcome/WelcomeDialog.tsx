import { useEffect, useRef, useState } from 'react';
import { createPortal } from 'react-dom';
import { useTranslation } from 'react-i18next';
import { X } from 'lucide-react';
import { appService } from '../../services/appService';
import { useWelcomeStore } from '../../stores/welcomeStore';
import { useNotificationStore } from '../../stores/notificationStore';
import './WelcomeDialog.css';

const CRAFTGINEER_URL = 'https://craftgineer.com';
const PRINTCUTCARVE_URL = 'https://printcutcarve.com';
const PHOTO_CONVERTER_POSTER = '/welcome/craftgineer-photo-converter-poster.png';
const CLOCKLAB_POSTER = '/welcome/craftgineer-clocklab-poster.png';
const VECTOR_STUDIO_POSTER = '/welcome/craftgineer-vector-studio-poster.png';

interface PromoVideoTileProps {
  name: string;
  outcome: string;
  src: string;
  poster: string;
  reducedMotion: boolean;
}

function PromoVideoTile({ name, outcome, src, poster, reducedMotion }: PromoVideoTileProps) {
  const videoRef = useRef<HTMLVideoElement>(null);

  useEffect(() => {
    const video = videoRef.current;
    if (!video) return;

    if (reducedMotion) {
      video.pause();
      video.currentTime = 0;
      return;
    }

    const playAttempt = video.play();
    if (playAttempt) {
      void playAttempt.catch(() => {
        // The poster remains visible if the webview blocks autoplay.
      });
    }
  }, [reducedMotion]);

  return (
    <figure className="welcome-cg-tool">
      <div className="welcome-cg-media">
        <video
          ref={videoRef}
          src={src}
          poster={poster}
          autoPlay={!reducedMotion}
          muted
          loop
          playsInline
          preload="metadata"
          disablePictureInPicture
          aria-hidden="true"
        />
      </div>
      <figcaption className="welcome-cg-caption">
        <span className="welcome-cg-name">{name}</span>
        <span className="welcome-cg-outcome">{outcome}</span>
      </figcaption>
    </figure>
  );
}

function useReducedMotion() {
  const [reducedMotion, setReducedMotion] = useState(
    () => window.matchMedia?.('(prefers-reduced-motion: reduce)').matches ?? false,
  );

  useEffect(() => {
    const query = window.matchMedia?.('(prefers-reduced-motion: reduce)');
    if (!query) return;

    const updatePreference = () => setReducedMotion(query.matches);
    updatePreference();
    query.addEventListener?.('change', updatePreference);
    return () => query.removeEventListener?.('change', updatePreference);
  }, []);

  return reducedMotion;
}

/**
 * Welcome / promo screen. Promotes the sister products with equal billing and
 * shows on every startup. It can be closed for the current session (X, Escape,
 * or clicking the backdrop) but has no permanent opt-out, so it returns on the
 * next launch. Opening a product keeps the panel open so both can be visited.
 */
export function WelcomeDialog() {
  const { t } = useTranslation();
  const overlayRef = useRef<HTMLDivElement>(null);
  const reducedMotion = useReducedMotion();
  const closeDialog = useWelcomeStore((s) => s.closeDialog);
  const pushNotification = useNotificationStore((s) => s.push);

  useEffect(() => {
    overlayRef.current?.focus();
  }, []);

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
      onKeyDown={(e) => {
        if (e.key === 'Escape') closeDialog();
      }}
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
                <img src="/welcome/craftgineer-mark.svg" alt="" />
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
                    <PromoVideoTile
                      name={t('dialog.welcome.photo_converter_name')}
                      outcome={t('dialog.welcome.photo_converter_outcome')}
                      src="/welcome/craftgineer-photo-converter.mp4"
                      poster={PHOTO_CONVERTER_POSTER}
                      reducedMotion={reducedMotion}
                    />
                    <PromoVideoTile
                      name={t('dialog.welcome.clocklab_name')}
                      outcome={t('dialog.welcome.clocklab_outcome')}
                      src="/welcome/craftgineer-clocklab.mp4"
                      poster={CLOCKLAB_POSTER}
                      reducedMotion={reducedMotion}
                    />
                    <PromoVideoTile
                      name={t('dialog.welcome.vector_studio_name')}
                      outcome={t('dialog.welcome.vector_studio_outcome')}
                      src="/welcome/craftgineer-vector-studio.mp4"
                      poster={VECTOR_STUDIO_POSTER}
                      reducedMotion={reducedMotion}
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
                <img src="/welcome/printcutcarve-mark.svg" alt="" />
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
                  <img src="/welcome/printcutcarve-pirate.jpg" alt="" />
                  <img src="/welcome/printcutcarve-wedding.jpg" alt="" />
                  <img src="/welcome/printcutcarve-boards.jpg" alt="" />
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
