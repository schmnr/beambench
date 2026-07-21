import { useTranslation } from 'react-i18next';
import type { OverrideAction } from '../../types/machine';
import { useMachineStore } from '../../stores/machineStore';
import { machineService } from '../../services/machineService';
import { useNotificationStore } from '../../stores/notificationStore';
import { wrapBackendError } from '../../i18n/errors';

const DECREASE_10_LABEL = '-10%';
const DECREASE_1_LABEL = '-1%';
const INCREASE_1_LABEL = '+1%';
const INCREASE_10_LABEL = '+10%';

export function OverrideControls() {
  const { t } = useTranslation();
  const machineStatus = useMachineStore((s) => s.machineStatus);
  const feedPercent = machineStatus?.feed_override ?? 100;
  const spindlePercent = machineStatus?.spindle_override ?? 100;

  const handleFeed = async (action: OverrideAction) => {
    try {
      await machineService.setFeedOverride(action);
    } catch (e) {
      useNotificationStore.getState().push(wrapBackendError(String(e)), 'error');
    }
  };

  const handleSpindle = async (action: OverrideAction) => {
    try {
      await machineService.setSpindleOverride(action);
    } catch (e) {
      useNotificationStore.getState().push(wrapBackendError(String(e)), 'error');
    }
  };

  const handleResetAll = async () => {
    try {
      await machineService.resetAllOverrides();
    } catch (e) {
      useNotificationStore.getState().push(wrapBackendError(String(e)), 'error');
    }
  };

  const btnClass =
    'px-1.5 py-0.5 text-xs rounded bg-bb-hover text-bb-text hover:bg-bb-border';

  return (
    <div className="flex flex-col gap-1.5 text-xs">
      <div className="text-bb-text-muted font-medium">{t('panels.machine.overrides.title')}</div>

      <div className="flex items-center gap-1">
        <span className="w-12 text-bb-text-muted">{t('panels.machine.overrides.feed')}</span>
        <button className={btnClass} onClick={() => handleFeed('decrease_10')}>
          {DECREASE_10_LABEL}
        </button>
        <button className={btnClass} onClick={() => handleFeed('decrease_1')}>
          {DECREASE_1_LABEL}
        </button>
        <button
          className={btnClass}
          onClick={() => handleFeed('reset')}
          title={t('panels.machine.overrides.reset_to_100')}
        >
          {feedPercent}%
        </button>
        <button className={btnClass} onClick={() => handleFeed('increase_1')}>
          {INCREASE_1_LABEL}
        </button>
        <button className={btnClass} onClick={() => handleFeed('increase_10')}>
          {INCREASE_10_LABEL}
        </button>
      </div>

      <div className="flex items-center gap-1">
        <span className="w-12 text-bb-text-muted">{t('panels.machine.overrides.power')}</span>
        <button className={btnClass} onClick={() => handleSpindle('decrease_10')}>
          {DECREASE_10_LABEL}
        </button>
        <button className={btnClass} onClick={() => handleSpindle('decrease_1')}>
          {DECREASE_1_LABEL}
        </button>
        <button
          className={btnClass}
          onClick={() => handleSpindle('reset')}
          title={t('panels.machine.overrides.reset_to_100')}
        >
          {spindlePercent}%
        </button>
        <button className={btnClass} onClick={() => handleSpindle('increase_1')}>
          {INCREASE_1_LABEL}
        </button>
        <button className={btnClass} onClick={() => handleSpindle('increase_10')}>
          {INCREASE_10_LABEL}
        </button>
      </div>

      <button
        className="text-xs px-2 py-0.5 rounded bg-bb-hover text-bb-text-muted hover:bg-bb-border hover:text-bb-text"
        onClick={handleResetAll}
      >
        {t('panels.machine.overrides.reset_all')}
      </button>
    </div>
  );
}
