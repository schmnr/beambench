//! Real serial transport backed by the `serialport` crate.

use crate::error::SerialError;
use crate::port_list::list_available_ports;
use crate::telemetry::{record_rx, record_tx, reset_serial_traffic};
use crate::transport::SerialTransport;
use beambench_common::machine::PortInfo;
use std::io::{Read, Write};
use std::time::Duration;
use tracing::{debug, warn};

/// Real serial transport wrapping a hardware serial port.
pub struct RealSerialTransport {
    port_name: String,
    baud_rate: u32,
    toggle_dtr_on_open: bool,
    usb_identity: Option<(u16, u16)>,
    port: Option<Box<dyn serialport::SerialPort>>,
    line_buffer: String,
}

impl RealSerialTransport {
    pub fn new(port_name: &str, baud_rate: u32) -> Self {
        Self::with_dtr(port_name, baud_rate, true)
    }

    /// Open a serial device without toggling DTR.
    ///
    /// Some GRBL-compatible OEM devices, including LaserPecker's published
    /// Lbrn profiles, explicitly disable DTR. These devices are validated
    /// with a fresh status query instead of resetting them to obtain a banner.
    pub fn new_without_dtr(port_name: &str, baud_rate: u32) -> Self {
        Self::with_dtr(port_name, baud_rate, false)
    }

    fn with_dtr(port_name: &str, baud_rate: u32, toggle_dtr_on_open: bool) -> Self {
        let usb_identity = list_available_ports().ok().and_then(|ports| {
            ports
                .iter()
                .find(|port| port.port_name == port_name)
                .and_then(port_usb_identity)
        });
        Self {
            port_name: port_name.to_string(),
            baud_rate,
            toggle_dtr_on_open,
            usb_identity,
            port: None,
            line_buffer: String::new(),
        }
    }

    fn rediscover_usb_port(&mut self) {
        let Ok(ports) = list_available_ports() else {
            return;
        };
        if let Some(port) = ports.iter().find(|port| port.port_name == self.port_name) {
            self.usb_identity = self.usb_identity.or_else(|| port_usb_identity(port));
            return;
        }
        let Some(usb_identity) = self.usb_identity else {
            return;
        };
        let Some(replacement) = replacement_usb_port(&self.port_name, usb_identity, &ports) else {
            return;
        };
        debug!(old_port = %self.port_name, new_port = %replacement, "USB serial port name changed");
        self.port_name = replacement;
    }
}

fn port_usb_identity(port: &PortInfo) -> Option<(u16, u16)> {
    Some((port.vid?, port.pid?))
}

fn port_name_family(port_name: &str) -> &str {
    port_name.trim_end_matches(|character: char| character.is_ascii_digit())
}

fn replacement_usb_port(
    old_port_name: &str,
    usb_identity: (u16, u16),
    ports: &[PortInfo],
) -> Option<String> {
    let matches = ports
        .iter()
        .filter(|port| port_usb_identity(port) == Some(usb_identity))
        .collect::<Vec<_>>();
    let same_family = matches
        .iter()
        .filter(|port| port_name_family(&port.port_name) == port_name_family(old_port_name))
        .collect::<Vec<_>>();
    match same_family.as_slice() {
        [port] => Some(port.port_name.clone()),
        [] if matches.len() == 1 => Some(matches[0].port_name.clone()),
        _ => None,
    }
}

fn map_open_error_for_platform(
    port_name: &str,
    error: serialport::Error,
    windows: bool,
) -> SerialError {
    let detail = error.to_string();
    let lower_detail = detail.to_lowercase();
    if windows && matches!(error.kind(), serialport::ErrorKind::NoDevice) {
        // serialport maps ERROR_ACCESS_DENIED, ERROR_FILE_NOT_FOUND, and
        // ERROR_PATH_NOT_FOUND to NoDevice on Windows, then localizes the
        // description. Do not inspect translated text. A single unavailable
        // message accurately covers both port contention and disconnection.
        return SerialError::PortUnavailable {
            port_name: port_name.to_string(),
            detail: detail.trim_end().trim_end_matches('.').to_string(),
        };
    }
    let is_access_denied = matches!(
        error.kind(),
        serialport::ErrorKind::Io(std::io::ErrorKind::PermissionDenied)
    ) || lower_detail.contains("access is denied")
        || lower_detail.contains("permission denied");

    if is_access_denied {
        // OS detail often ends in its own period ("Access is denied.");
        // trim it so the appended guidance sentence doesn't double up.
        let detail = detail.trim_end().trim_end_matches('.').to_string();
        return SerialError::AccessDenied {
            port_name: port_name.to_string(),
            detail,
        };
    }

    SerialError::ConnectionFailed(detail)
}

fn map_open_error(port_name: &str, error: serialport::Error) -> SerialError {
    map_open_error_for_platform(port_name, error, cfg!(windows))
}

impl SerialTransport for RealSerialTransport {
    fn open(&mut self) -> Result<(), SerialError> {
        if self.port.is_some() {
            return Err(SerialError::AlreadyOpen);
        }

        debug!(port = %self.port_name, baud = self.baud_rate, "Opening serial port");
        self.rediscover_usb_port();

        let port = serialport::new(&self.port_name, self.baud_rate)
            .timeout(Duration::from_millis(100))
            .open()
            .map_err(|e| map_open_error(&self.port_name, e))?;

        // Keep the previous session's traffic when an open fails. It is often
        // the only evidence left after a USB controller drops off the bus.
        reset_serial_traffic();
        self.port = Some(port);
        self.line_buffer.clear();

        // Drain any stale data sitting in the OS receive buffer from a
        // previous session *before* the DTR reset so we don't accidentally
        // consume the GRBL banner that arrives after the reset.
        if let Ok(stale) = self.read_available()
            && !stale.is_empty()
        {
            debug!("Drained {} stale bytes before DTR reset", stale.len());
        }

        if self.toggle_dtr_on_open {
            // Toggle DTR to reset Arduino-based GRBL controllers.
            // Pull DTR low then high — the falling edge triggers an MCU reset,
            // causing the bootloader to run and GRBL to emit its startup banner.
            if let Err(e) = self.set_dtr(false) {
                debug!("DTR toggle (low) failed, continuing: {e}");
            }
            std::thread::sleep(Duration::from_millis(50));
            if let Err(e) = self.set_dtr(true) {
                debug!("DTR toggle (high) failed, continuing: {e}");
            }
        }

        Ok(())
    }

    fn close(&mut self) -> Result<(), SerialError> {
        if self.port.is_none() {
            return Err(SerialError::NotOpen);
        }
        debug!(port = %self.port_name, "Closing serial port");
        self.port = None;
        self.line_buffer.clear();
        Ok(())
    }

    fn is_open(&self) -> bool {
        self.port.is_some()
    }

    fn write_bytes(&mut self, data: &[u8]) -> Result<usize, SerialError> {
        let port = self.port.as_mut().ok_or(SerialError::NotOpen)?;
        port.write_all(data)
            .map_err(|e| SerialError::WriteFailed(e.to_string()))?;
        record_tx(data);
        Ok(data.len())
    }

    fn write_line(&mut self, line: &str) -> Result<(), SerialError> {
        let data = format!("{}\n", line);
        self.write_bytes(data.as_bytes())?;
        self.flush()?;
        Ok(())
    }

    fn read_available(&mut self) -> Result<Vec<u8>, SerialError> {
        let port = self.port.as_mut().ok_or(SerialError::NotOpen)?;
        // A failed buffer query is not the same as an empty buffer. Windows
        // CH340 handles commonly surface a removed, reset, or invalidated
        // device here. Preserve that failure so the service can recheck the
        // connection and clear a stale "connected" session.
        let bytes_available = port.bytes_to_read().map_err(|error| {
            SerialError::IoError(std::io::Error::other(format!(
                "could not query {} input buffer: {error}",
                self.port_name
            )))
        })? as usize;
        if bytes_available == 0 {
            return Ok(Vec::new());
        }
        let mut buf = vec![0u8; bytes_available];
        match port.read(&mut buf) {
            Ok(n) => {
                buf.truncate(n);
                record_rx(&buf);
                Ok(buf)
            }
            Err(ref e) if e.kind() == std::io::ErrorKind::TimedOut => Ok(Vec::new()),
            Err(e) => Err(SerialError::IoError(e)),
        }
    }

    fn read_line(&mut self) -> Result<Option<String>, SerialError> {
        // Read any available bytes into the line buffer
        let new_bytes = self.read_available()?;
        if !new_bytes.is_empty() {
            match String::from_utf8(new_bytes) {
                Ok(s) => self.line_buffer.push_str(&s),
                Err(e) => {
                    warn!("Non-UTF8 data received, using lossy conversion");
                    let s = String::from_utf8_lossy(e.as_bytes()).to_string();
                    self.line_buffer.push_str(&s);
                }
            }
        }

        // Check if we have a complete line
        if let Some(newline_pos) = self.line_buffer.find('\n') {
            let line = self.line_buffer[..newline_pos]
                .trim_end_matches('\r')
                .to_string();
            self.line_buffer = self.line_buffer[newline_pos + 1..].to_string();
            Ok(Some(line))
        } else {
            Ok(None)
        }
    }

    fn flush(&mut self) -> Result<(), SerialError> {
        let port = self.port.as_mut().ok_or(SerialError::NotOpen)?;
        port.flush()
            .map_err(|e| SerialError::WriteFailed(e.to_string()))
    }

    fn set_dtr(&mut self, level: bool) -> Result<(), SerialError> {
        let port = self.port.as_mut().ok_or(SerialError::NotOpen)?;
        port.write_data_terminal_ready(level)
            .map_err(|e| SerialError::WriteFailed(e.to_string()))
    }

    fn port_name(&self) -> &str {
        &self.port_name
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::telemetry::{SERIAL_TRAFFIC_TEST_LOCK, recent_serial_traffic};

    fn usb_port(port_name: &str, vid: u16, pid: u16) -> PortInfo {
        PortInfo {
            port_name: port_name.to_string(),
            description: String::new(),
            manufacturer: String::new(),
            vid: Some(vid),
            pid: Some(pid),
        }
    }

    #[test]
    fn rediscovers_a_renamed_usb_port_in_the_same_endpoint_family() {
        let ports = vec![
            usb_port("/dev/cu.usbserial-110", 0x1a86, 0x7523),
            usb_port("/dev/tty.usbserial-110", 0x1a86, 0x7523),
        ];

        let replacement = replacement_usb_port("/dev/cu.usbserial-10", (0x1a86, 0x7523), &ports);

        assert_eq!(replacement.as_deref(), Some("/dev/cu.usbserial-110"));
    }

    #[test]
    fn refuses_an_ambiguous_usb_port_replacement() {
        let ports = vec![
            usb_port("/dev/ttyUSB1", 0x1a86, 0x7523),
            usb_port("/dev/ttyUSB2", 0x1a86, 0x7523),
        ];

        let replacement = replacement_usb_port("/dev/ttyUSB0", (0x1a86, 0x7523), &ports);

        assert_eq!(replacement, None);
    }

    #[test]
    fn failed_open_preserves_previous_serial_traffic() {
        let _guard = SERIAL_TRAFFIC_TEST_LOCK.lock().unwrap();
        reset_serial_traffic();
        record_tx(b"emergency-stop-evidence");
        let mut transport = RealSerialTransport::new("beambench-missing-serial-port", 115_200);

        assert!(transport.open().is_err());

        let traffic = recent_serial_traffic();
        assert!(traffic.tx_ascii.contains("emergency-stop-evidence"));
    }

    #[test]
    fn localized_windows_no_device_error_gets_actionable_message() {
        let error = serialport::Error::new(serialport::ErrorKind::NoDevice, "Accès refusé.");

        let mapped = map_open_error_for_platform("COM7", error, true);

        assert!(matches!(
            mapped,
            SerialError::PortUnavailable { ref port_name, .. } if port_name == "COM7"
        ));
        let message = mapped.to_string();
        assert!(message.contains("[serial_port_unavailable]"));
        assert!(message.contains("Could not open COM7"));
        assert!(message.contains("another application"));
        assert!(message.contains("controller may have been disconnected"));
        assert!(
            !message.contains(".."),
            "OS detail's trailing period must be trimmed: {message}"
        );
    }

    #[test]
    fn permission_denied_open_error_gets_actionable_message() {
        let error = serialport::Error::new(
            serialport::ErrorKind::Io(std::io::ErrorKind::PermissionDenied),
            "Permission denied",
        );

        let mapped = map_open_error_for_platform("/dev/ttyUSB0", error, false);

        assert!(matches!(mapped, SerialError::AccessDenied { .. }));
        let message = mapped.to_string();
        assert!(message.contains("access denied opening /dev/ttyUSB0"));
        if cfg!(windows) {
            assert!(message.contains("Another application may already be using this serial port"));
        } else {
            assert!(message.contains("add your user to the dialout group"));
        }
    }

    #[test]
    fn unrelated_open_error_preserves_raw_detail() {
        let error = serialport::Error::new(serialport::ErrorKind::NoDevice, "No such file");

        let mapped = map_open_error_for_platform("/dev/ttyUSB8", error, false);

        assert!(matches!(mapped, SerialError::ConnectionFailed(_)));
        assert_eq!(mapped.to_string(), "connection failed: No such file");
    }
}
