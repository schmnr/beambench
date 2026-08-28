import { describe, expect, it } from 'vitest';
import { localizeImportWarning, wrapBackendError } from './errors';

describe('wrapBackendError', () => {
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
});
