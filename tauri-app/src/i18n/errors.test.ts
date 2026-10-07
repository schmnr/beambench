import { describe, expect, it } from 'vitest';
import { localizeImportWarning, wrapBackendError } from './errors';
import i18n, { SUPPORTED_LOCALES } from './index';
import { useAppStore } from '../stores/appStore';
import type { AppSettings } from '../types/commands';

describe('wrapBackendError', () => {
  it.each(SUPPORTED_LOCALES)('localizes release safety messages in %s', async (locale) => {
    const previousLanguage = i18n.language;
    try {
      await i18n.changeLanguage(locale);
      for (const key of [
        'job_preparation_stopped',
        'sleep_protection_failed',
        'raster_plan_too_complex',
        'settings_file_recovered',
        'layers_migrated_on_open',
        'edit_invalid_geometry',
        'edit_internal_error',
      ]) {
        const translated = wrapBackendError(`Error: [${key}] Backend diagnostic detail`);
        expect(translated).toBe(i18n.t(`errors.${key}`));
        expect(translated).not.toContain('Backend diagnostic detail');
        if (locale !== 'en') {
          expect(translated).not.toBe(i18n.t(`errors.${key}`, { lng: 'en' }));
        }
      }
      const missing = wrapBackendError('Error: [project_file_missing] The project file was moved or deleted: C:\\gone.lzrproj');
      expect(missing).toContain(i18n.t('menus.recent_projects.label'));
      expect(missing).toContain(i18n.t('menus.file.open'));
      expect(missing).not.toMatch(/\{(recent|open)\}/);
      expect(missing).not.toContain('gone.lzrproj');
      if (locale !== 'en') {
        expect(i18n.getResource(locale, 'translation', 'errors.project_file_missing')).not.toBe(
          i18n.getResource('en', 'translation', 'errors.project_file_missing'),
        );
      }
      for (const key of ['controller_choice.transport_network', 'panels.machine.laser.frame_speed']) {
        if (locale !== 'en') expect(i18n.t(key)).not.toBe(i18n.t(key, { lng: 'en' }));
      }
    } finally {
      await i18n.changeLanguage(previousLanguage);
    }
  });

  it('names the newer Beam Bench version for projects it cannot safely open', () => {
    expect(
      wrapBackendError(
        'Failed to open project: validation error: [project_too_new] This project uses file format 2.0 from Beam Bench 0.4.1. Update Beam Bench to open it.',
      ),
    ).toBe(i18n.t('errors.project_too_new', { version: '0.4.1' }));
    expect(
      wrapBackendError(
        '[project_from_newer_app] This project was saved by Beam Bench 0.3.0. Saving it here may drop settings this version does not support.',
      ),
    ).toBe(i18n.t('errors.project_from_newer_app', { version: '0.3.0' }));
    expect(i18n.t('errors.project_too_new', { version: '0.4.1' })).toContain('0.4.1');
  });

  it('names the object that blocked a save with an invalid value', () => {
    expect(
      wrapBackendError(
        "Save failed: validation error: [project_invalid_value] 'Logo outline' has an invalid size or position, so the project was not saved. Undo the last change to it, or delete it, then save again.",
      ),
    ).toBe(i18n.t('errors.project_invalid_value_object', { name: 'Logo outline' }));
    expect(
      wrapBackendError('[project_invalid_value] The project has an invalid setting value, so it was not saved.'),
    ).toBe(i18n.t('errors.project_invalid_value'));
  });

  it('translates the machine-zero homing gate into a direct instruction', () => {
    expect(
      wrapBackendError('Machine-zero moves require homing in the current session first'),
    ).toBe('Home the machine first to use machine zero.');
    expect(
      wrapBackendError('Error: Machine-zero moves require homing in the current session first'),
    ).toBe('Home the machine first to use machine zero.');
    expect(
      wrapBackendError('Operation failed: Error: Machine-zero moves require homing in the current session first'),
    ).toBe('Home the machine first to use machine zero.');
  });

  it('keeps unknown backend errors wrapped with their original detail', () => {
    expect(wrapBackendError('Unexpected backend detail')).toBe(
      'Operation failed: Unexpected backend detail',
    );
  });

  it('uses the stable serial error code even when Windows localizes the detail', () => {
    expect(
      wrapBackendError(
        'transport error: [serial_port_unavailable] Could not open COM5: Accès refusé. The port may be in use by another application or the controller may have been disconnected.',
      ),
    ).toBe(
      'Could not open COM5. Another application may be using this port, or the controller may have been disconnected. Close other laser or serial software, reconnect the controller, and try again.',
    );
  });

  it('uses the stable Lihuiyu driver code instead of exposing the backend exception', () => {
    expect(
      wrapBackendError(
        '[lihuiyu_incompatible_windows_driver] the CH341 device uses Windows driver CH341PAR',
      ),
    ).toBe(
      'This Lihuiyu controller is using an incompatible Windows USB driver. Beam Bench requires WinUSB for device 1a86:5512. Change the driver, reconnect the controller, then refresh the USB list. Changing the driver may prevent vendor software from using the controller until its original driver is restored.',
    );
  });

  it('explains that an unknown Ruida probe did not send motion', () => {
    expect(
      wrapBackendError(
        '[ruida_unknown_variant] Beam Bench found an unrecognized Ruida controller using read-only queries (card ID 0x1234).',
      ),
    ).toBe(
      'Beam Bench found a Ruida controller it does not yet recognize. No file or motion command was sent. Submit a bug report and include the controller model and firmware shown on its panel.',
    );
  });

  it('explains an inconclusive Ruida probe without exposing its backend detail', () => {
    expect(
      wrapBackendError(
        '[ruida_probe_inconclusive] Ruida adapter validation failed: reply timed out',
      ),
    ).toBe(
      'Beam Bench could not complete the read-only Ruida compatibility check. No file or motion command was sent. Check the network connection, then submit a bug report if it happens again.',
    );
  });

  it('turns an oversized raster plan into actionable localized guidance', () => {
    expect(
      wrapBackendError(
        "Plan generation failed: Invalid planner settings: [raster_plan_too_complex] Image 'photo' creates more than 1000000 raster burn runs",
      ),
    ).toBe(
      'This image creates too many individual raster moves at its current size and DPI. Reduce the DPI or physical size, or choose a less fragmented image mode, then generate the preview again.',
    );
  });

  it('keeps internal safety markers out of user-facing warnings', () => {
    expect(
      wrapBackendError(
        '[emergency_stop_unconfirmed] Stop delivery was not confirmed. Use the physical stop.',
      ),
    ).toBe(
      'Operation failed: Stop delivery was not confirmed. Use the physical stop.',
    );
    expect(
      wrapBackendError(
        '[controller_connection_lost] Automatic controller rechecks failed.',
      ),
    ).toBe('Operation failed: Automatic controller rechecks failed.');
  });

  it('localizes DXF empty and partial import feedback', () => {
    expect(wrapBackendError('DXF import found no usable 2D vector geometry.')).toBe(
      'The DXF file contains no usable 2D vector geometry.',
    );
    expect(
      wrapBackendError(
        'DXF import found no usable 2D vector geometry. Unsupported or malformed entities: 1 POLYLINE.',
      ),
    ).toBe(
      'The DXF file contains no usable 2D vector geometry. Unsupported or malformed entities: 1 POLYLINE.',
    );
    expect(
      localizeImportWarning(
        'DXF import skipped unsupported or malformed entities: 1 TEXT.',
      ),
    ).toBe('Some DXF entities could not be imported: 1 TEXT.');
  });

  it('translates connection failures and never shows their tags', () => {
    const silent = wrapBackendError(
      '[serial_open_no_response] The serial port opened, but no supported controller replied. GRBL-family baud rates: 115200.',
    );
    expect(silent).toContain('the laser did not answer');
    expect(silent).not.toContain('[');
    const unknown = wrapBackendError(
      'Operation failed: [serial_protocol_unrecognized] The serial port returned data, but no supported controller protocol was recognized.',
    );
    expect(unknown).toContain('did not recognize its controller');
    expect(unknown).not.toContain('[');
    expect(wrapBackendError('The network controller did not return a GRBL status report')).toContain(
      'did not answer as a GRBL laser',
    );
    expect(wrapBackendError('No active machine profile')).toBe(
      'No machine is set up yet. Add or select a machine profile first.',
    );
  });

  it('never shows an unknown leading tag', () => {
    expect(wrapBackendError('[some_new_code] Something failed.')).toBe(
      'Operation failed: Something failed.',
    );
  });

  it('shows raster overscan export errors in the display unit', () => {
    useAppStore.setState({ settings: { display_unit: 'inches' } as AppSettings });
    const message = wrapBackendError(
      '[raster_motion_off_bed] Raster motion spans -12.7 to 254.0mm on the 0 to 254mm X axis (12.7mm of overscan and scanning offset beyond the burn area). Reduce overscan or move the design further from the bed edge.',
    );
    expect(message).toContain('-0.5 to 10 in on the 0 to 10 in X axis');
    expect(message).toContain('add 0.5 in');
    useAppStore.setState({ settings: null });
  });

  it('gives safety guidance in the user language', async () => {
    await i18n.changeLanguage('de');
    const message = wrapBackendError('[emergency_stop_unconfirmed] Controller did not confirm the stop.');
    expect(message).toContain('Not-Aus');
    expect(message).toContain('Controller did not confirm the stop.');
    await i18n.changeLanguage('en');
  });
});
