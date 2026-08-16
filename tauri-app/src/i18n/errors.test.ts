import { describe, expect, it } from 'vitest';
import { wrapBackendError } from './errors';

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
});
