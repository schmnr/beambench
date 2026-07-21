import { useTranslation } from 'react-i18next';
import { useMachineStore } from '../../stores/machineStore';
import { useAppStore } from '../../stores/appStore';
import { mmToDisplay, roundDisplayLength, lengthUnitLabel } from '../../utils/lengthUnits';
import { formatSpeedForDisplay, speedUnitLabel } from '../../utils/speedUnits';
import { RUN_STATE_TEXT_CLASSES } from './stateColors';

export function StatusDisplay() {
  const { t } = useTranslation();
  const machineStatus = useMachineStore((s) => s.machineStatus);
  const displayUnit = useAppStore((s) => s.settings?.display_unit) ?? 'mm';
  const speedTimeUnit = useAppStore((s) => s.settings?.speed_time_unit) ?? 'minutes';

  if (!machineStatus) {
    return null;
  }

  const {
    run_state,
    work_position,
    feed_rate,
    spindle_speed,
    feed_override,
    spindle_override,
    rapid_override,
  } = machineStatus;

  const runStateClass = RUN_STATE_TEXT_CLASSES[run_state];

  return (
    <div className="space-y-3">
      {/* Run State */}
      <div className="flex items-center gap-2">
        <span className="text-bb-text-muted text-xs">{t('panels.machine.status.status_label')}</span>
        <span className={`text-sm font-semibold uppercase ${runStateClass}`}>
          {t(`panels.machine.status.run_state.${run_state}`)}
        </span>
      </div>

      {/* Work Position */}
      <div>
        <div className="text-bb-text-muted text-xs mb-1">{t('panels.machine.status.work_position')} ({lengthUnitLabel(displayUnit)})</div>
        <div className="grid grid-cols-3 gap-2 font-mono text-sm text-bb-text">
          <div>
            <span className="text-bb-text-muted">{t('panels.machine.status.axis_x')}</span>{' '}
            {roundDisplayLength(mmToDisplay(work_position.x, displayUnit), displayUnit)}
          </div>
          <div>
            <span className="text-bb-text-muted">{t('panels.machine.status.axis_y')}</span>{' '}
            {roundDisplayLength(mmToDisplay(work_position.y, displayUnit), displayUnit)}
          </div>
          <div>
            <span className="text-bb-text-muted">{t('panels.machine.status.axis_z')}</span>{' '}
            {roundDisplayLength(mmToDisplay(work_position.z, displayUnit), displayUnit)}
          </div>
        </div>
      </div>

      {/* Feed Rate & GRBL laser S output */}
      <div className="flex gap-4 text-xs text-bb-text-muted">
        <div>
          {t('panels.machine.status.feed_label')} <span className="text-bb-text">{formatSpeedForDisplay(feed_rate, displayUnit, speedTimeUnit)}</span> {speedUnitLabel(displayUnit, speedTimeUnit)}
        </div>
        <div>
          {t('panels.machine.status.spindle_label')} <span className="text-bb-text">{spindle_speed}</span>
        </div>
      </div>

      {/* Override Percentages */}
      <div className="text-xs text-bb-text-muted">
        {t('panels.machine.status.overrides_label')}{' '}
        <span className="text-bb-text">
          {t('panels.machine.status.feed_short')}{feed_override}% {t('panels.machine.status.spindle_short')}{spindle_override}% {t('panels.machine.status.rapid_short')}{rapid_override}%
        </span>
      </div>
    </div>
  );
}
