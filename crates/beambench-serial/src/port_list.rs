//! Serial port enumeration.

use crate::error::SerialError;
use beambench_common::machine::PortInfo;

/// List all available serial ports on the system.
pub fn list_available_ports() -> Result<Vec<PortInfo>, SerialError> {
    let ports =
        serialport::available_ports().map_err(|e| SerialError::ConnectionFailed(e.to_string()))?;

    Ok(ports
        .into_iter()
        .map(|p| {
            let (description, manufacturer, vid, pid) = match p.port_type {
                serialport::SerialPortType::UsbPort(info) => (
                    info.product.unwrap_or_default(),
                    info.manufacturer.unwrap_or_default(),
                    Some(info.vid),
                    Some(info.pid),
                ),
                serialport::SerialPortType::PciPort => {
                    ("PCI Serial".to_string(), String::new(), None, None)
                }
                serialport::SerialPortType::BluetoothPort => {
                    ("Bluetooth Serial".to_string(), String::new(), None, None)
                }
                serialport::SerialPortType::Unknown => (String::new(), String::new(), None, None),
            };

            PortInfo {
                port_name: p.port_name,
                description,
                manufacturer,
                vid,
                pid,
            }
        })
        .collect())
}
