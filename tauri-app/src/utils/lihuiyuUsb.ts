import type { LihuiyuUsbDeviceInfo } from '../types/machine';

export function lihuiyuUsbDeviceId(device: LihuiyuUsbDeviceInfo): string {
  if (device.port_numbers.length === 0) {
    return `usb-bus-${device.bus_id}-address-${device.device_address}`;
  }
  return `usb-bus-${device.bus_id}-ports-${device.port_numbers.join('.')}`;
}

export function lihuiyuUsbDeviceLabel(device: LihuiyuUsbDeviceInfo): string {
  const name = device.product?.trim()
    || device.manufacturer?.trim()
    || 'CH341 USB';
  return `${name} — ${lihuiyuUsbDeviceId(device)}`;
}
