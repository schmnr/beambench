import { useTranslation } from 'react-i18next';
import { Toggle } from '../shared/Toggle';

export function HomeOnConnectSetting({ value, onChange }: {
  value: boolean;
  onChange: (value: boolean) => void;
}) {
  const { t } = useTranslation();
  return (
    <div className="space-y-1.5 text-xs">
      <Toggle label={t('dialog.device_settings.home_on_connect')} checked={value} onChange={onChange} />
      <p className="text-bb-text-muted">{t('dialog.device_settings.home_on_connect_help')}</p>
      <p className="text-bb-text-muted">{t('dialog.device_settings.homing_firmware_help')}</p>
    </div>
  );
}
