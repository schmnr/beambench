import i18n from './index';
import type { PreflightDetail } from '../types/machine';
import type { StartFromMode } from '../types/project';
import { lengthUnitLabel, mmToDisplay, type DisplayUnit } from '../utils/lengthUnits';

/** A millimetre length in the user's unit and number format. */
export function formatLength(mm: number, unit: DisplayUnit, mmDigits = 1): string {
  return new Intl.NumberFormat(i18n.resolvedLanguage ?? i18n.language, {
    maximumFractionDigits: unit === 'inches' ? 2 : mmDigits,
  }).format(mmToDisplay(mm, unit));
}

/** Raster overscan running off the bed, in the user's language and unit. */
export function rasterMotionText(
  motion: { axis: string; lo: number; hi: number; limit: number; margin: number },
  unit: DisplayUnit,
  startFrom: StartFromMode,
): string {
  return i18n.t('dialog.preflight.messages.raster_motion_exceeds', {
    startFrom,
    lo: formatLength(motion.lo, unit),
    hi: formatLength(motion.hi, unit),
    limit: formatLength(motion.limit, unit, 0),
    axis: motion.axis,
    margin: formatLength(motion.margin, unit),
    unit: lengthUnitLabel(unit),
  });
}

/** Show a preflight check's values in the user's language and unit. */
export function preflightDetailText(
  detail: PreflightDetail,
  passed: boolean,
  unit: DisplayUnit,
  startFrom: StartFromMode,
): string {
  switch (detail.kind) {
    case 'session_state':
      return detail.state === 'ready'
        ? i18n.t('dialog.preflight.messages.connected_ready')
        : i18n.t('dialog.preflight.messages.session_state', {
            state: i18n.t(`status.connection.${detail.state}`, { defaultValue: detail.state }),
          });
    case 'machine_state':
      return detail.state === 'idle'
        ? i18n.t('dialog.preflight.messages.machine_idle')
        : i18n.t('dialog.preflight.messages.machine_state', {
            state: i18n.t(`panels.machine.status.run_state.${detail.state}`, {
              defaultValue: detail.state,
            }),
          });
    case 'segment_count':
      return i18n.t(
        `dialog.preflight.messages.segment_count_${detail.count === 1 ? 'one' : 'other'}`,
        { count: detail.count },
      );
    case 'plan_bounds':
      return i18n.t(
        passed
          ? 'dialog.preflight.messages.plan_bounds_fit'
          : 'dialog.preflight.messages.plan_bounds_exceed',
        {
          minX: formatLength(detail.min_x, unit),
          minY: formatLength(detail.min_y, unit),
          maxX: formatLength(detail.max_x, unit),
          maxY: formatLength(detail.max_y, unit),
          width: formatLength(detail.bed_width, unit, 0),
          height: formatLength(detail.bed_height, unit, 0),
          unit: lengthUnitLabel(unit),
        },
      );
    case 'raster_motion':
      return rasterMotionText(detail, unit, startFrom);
  }
}
