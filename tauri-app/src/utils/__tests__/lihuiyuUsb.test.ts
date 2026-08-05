import { describe, expect, it } from 'vitest';
import type { LihuiyuUsbDeviceInfo } from '../../types/machine';
import { lihuiyuUsbHasIncompatibleWindowsDriver } from '../lihuiyuUsb';

function device(windowsDriverCompatible: boolean | null): LihuiyuUsbDeviceInfo {
  return {
    bus_id: '20',
    device_address: 7,
    port_numbers: [1, 3],
    vendor_id: 0x1a86,
    product_id: 0x5512,
    manufacturer: null,
    product: 'M2 Nano',
    serial_number: null,
    has_required_bulk_endpoints: true,
    driver: windowsDriverCompatible === false ? 'CH341PAR' : 'WinUSB',
    windows_driver_compatible: windowsDriverCompatible,
  };
}

describe('lihuiyuUsbHasIncompatibleWindowsDriver', () => {
  it('only blocks devices the backend positively identifies as incompatible', () => {
    expect(lihuiyuUsbHasIncompatibleWindowsDriver(device(false))).toBe(true);
    expect(lihuiyuUsbHasIncompatibleWindowsDriver(device(true))).toBe(false);
    expect(lihuiyuUsbHasIncompatibleWindowsDriver(device(null))).toBe(false);
  });
});
