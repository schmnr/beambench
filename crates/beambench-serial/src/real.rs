//! Real serial transport backed by the `serialport` crate.

use crate::error::SerialError;
use crate::telemetry::{record_rx, record_tx, reset_serial_traffic};
use crate::transport::SerialTransport;
use std::io::{Read, Write};
use std::time::Duration;
use tracing::{debug, warn};

/// Real serial transport wrapping a hardware serial port.
pub struct RealSerialTransport {
    port_name: String,
    baud_rate: u32,
    toggle_dtr_on_open: bool,
    port: Option<Box<dyn serialport::SerialPort>>,
    line_buffer: String,
}

impl RealSerialTransport {
    pub fn new(port_name: &str, baud_rate: u32) -> Self {
        Self {
            port_name: port_name.to_string(),
            baud_rate,
            toggle_dtr_on_open: true,
            port: None,
            line_buffer: String::new(),
        }
    }

    /// Open a serial device without toggling DTR.
    ///
    /// Some GRBL-compatible OEM devices, including LaserPecker's published
    /// Lbrn profiles, explicitly disable DTR. These devices are validated
    /// with a fresh status query instead of resetting them to obtain a banner.
    pub fn new_without_dtr(port_name: &str, baud_rate: u32) -> Self {
        Self {
            port_name: port_name.to_string(),
            baud_rate,
            toggle_dtr_on_open: false,
            port: None,
            line_buffer: String::new(),
        }
    }
}

fn map_open_error(port_name: &str, error: serialport::Error) -> SerialError {
    let detail = error.to_string();
    let lower_detail = detail.to_lowercase();
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

impl SerialTransport for RealSerialTransport {
    fn open(&mut self) -> Result<(), SerialError> {
        if self.port.is_some() {
            return Err(SerialError::AlreadyOpen);
        }

        debug!(port = %self.port_name, baud = self.baud_rate, "Opening serial port");
        reset_serial_traffic();

        let port = serialport::new(&self.port_name, self.baud_rate)
            .timeout(Duration::from_millis(100))
            .open()
            .map_err(|e| map_open_error(&self.port_name, e))?;

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
        let bytes_available = port.bytes_to_read().unwrap_or(0) as usize;
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

    // The guidance sentence is chosen at compile time: Windows builds blame
    // port contention, Unix builds blame device permissions.
    fn expected_guidance() -> &'static str {
        if cfg!(windows) {
            "Another application may already be using this serial port"
        } else {
            "add your user to the dialout group"
        }
    }

    #[test]
    fn windows_access_denied_open_error_gets_actionable_message() {
        let error = serialport::Error::new(serialport::ErrorKind::NoDevice, "Access is denied.");

        let mapped = map_open_error("COM7", error);

        assert!(matches!(
            mapped,
            SerialError::AccessDenied { ref port_name, .. } if port_name == "COM7"
        ));
        let message = mapped.to_string();
        assert!(message.contains("access denied opening COM7"));
        assert!(message.contains(expected_guidance()));
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

        let mapped = map_open_error("/dev/ttyUSB0", error);

        assert!(matches!(mapped, SerialError::AccessDenied { .. }));
        let message = mapped.to_string();
        assert!(message.contains("access denied opening /dev/ttyUSB0"));
        assert!(message.contains(expected_guidance()));
    }

    #[test]
    fn unrelated_open_error_preserves_raw_detail() {
        let error = serialport::Error::new(serialport::ErrorKind::NoDevice, "No such file");

        let mapped = map_open_error("COM8", error);

        assert!(matches!(mapped, SerialError::ConnectionFailed(_)));
        assert_eq!(mapped.to_string(), "connection failed: No such file");
    }
}
