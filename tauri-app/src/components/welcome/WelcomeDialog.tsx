import { useEffect, useRef } from 'react';
import { createPortal } from 'react-dom';
import { useTranslation } from 'react-i18next';
import { X } from 'lucide-react';
import { appService } from '../../services/appService';
import { useWelcomeStore } from '../../stores/welcomeStore';
import { useNotificationStore } from '../../stores/notificationStore';
import { ProductCard } from './ProductCard';

const CRAFTGINEER_URL = 'https://craftgineer.com';
const PRINTCUTCARVE_URL = 'https://printcutcarve.com';

/**
 * Welcome / promo screen. Promotes the sister products with equal billing and
 * shows on every startup. It can be closed for the current session (X, Escape,
 * or clicking the backdrop) but has no permanent opt-out, so it returns on the
 * next launch. Opening a product keeps the panel open so both can be visited.
 */
export function WelcomeDialog() {
  const { t } = useTranslation();
  const overlayRef = useRef<HTMLDivElement>(null);
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
      className="fixed inset-0 bg-black/50 z-[9000] flex items-center justify-center"
      onKeyDown={(e) => {
        if (e.key === 'Escape') closeDialog();
      }}
      onClick={(e) => {
        if (e.target === e.currentTarget) closeDialog();
      }}
    >
      <div className="relative bg-bb-panel border border-bb-border rounded-lg shadow-xl w-[640px] max-w-[92vw] max-h-[90vh] overflow-y-auto p-6">
        <button
          onClick={closeDialog}
          aria-label={t('dialog.welcome.close_aria')}
          className="absolute top-3 right-3 text-bb-text-muted hover:text-bb-text transition-colors"
        >
          <X size={18} />
        </button>

        <div className="text-center mb-5 px-6">
          <h2 id="welcome-dialog-title" className="text-xl font-semibold text-bb-text leading-none">
            {t('dialog.welcome.title')}
          </h2>
          <p className="text-sm text-bb-text-muted mt-2">{t('dialog.welcome.subtitle')}</p>
        </div>

        <div className="flex gap-4">
          <ProductCard
            name={t('dialog.welcome.craftgineer_name')}
            tagline={t('dialog.welcome.craftgineer_tagline')}
            description={t('dialog.welcome.craftgineer_description')}
            buttonLabel={t('dialog.welcome.craftgineer_visit')}
            onVisit={() => openExternal(CRAFTGINEER_URL)}
          />
          <ProductCard
            name={t('dialog.welcome.printcutcarve_name')}
            tagline={t('dialog.welcome.printcutcarve_tagline')}
            description={t('dialog.welcome.printcutcarve_description')}
            buttonLabel={t('dialog.welcome.printcutcarve_visit')}
            onVisit={() => openExternal(PRINTCUTCARVE_URL)}
          />
        </div>
      </div>
    </div>,
    document.body,
  );
}
