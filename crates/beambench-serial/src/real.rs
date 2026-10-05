//! Real serial transport backed by the `serialport` crate.

use crate::error::SerialError;
use crate::telemetry::{record_rx, record_tx};
use crate::transport::SerialTransport;
use std::collections::HashSet;
use std::io::{Read, Write};
use std::time::Duration;
use tracing::{debug, warn};

/// USB identity used to re-find a port renamed after a replug. VID/PID alone
/// is shared by every board with the same USB chip (CH340 is in most hobby
/// lasers and many 3D printers), so the serial number is compared when the
/// device has one.
#[derive(Debug, Clone, PartialEq, Eq)]
struct UsbIdentity {
    vid: u16,
    pid: u16,
    serial: Option<String>,
}

#[derive(Debug, Clone)]
struct UsbPort {
    name: String,
    identity: Option<UsbIdentity>,
}

fn usb_ports() -> Option<Vec<UsbPort>> {
    let ports = serialport::available_ports().ok()?;
    Some(
        ports
            .into_iter()
            .map(|port| {
                let identity = match port.port_type {
                    serialport::SerialPortType::UsbPort(info) => Some(UsbIdentity {
                        vid: info.vid,
                        pid: info.pid,
                        serial: info.serial_number.filter(|serial| !serial.trim().is_empty()),
                    }),
                    _ => None,
                };
                UsbPort {
                    name: port.port_name,
                    identity,
                }
            })
            .collect(),
    )
}

/// Bytes held while waiting for a newline. GRBL lines are under 128 bytes;
/// anything far beyond that is a wrong baud rate or a non-line protocol.
const MAX_LINE_BUFFER_BYTES: usize = 64 * 1024;

/// Real serial transport wrapping a hardware serial port.
pub struct RealSerialTransport {
    port_name: String,
    baud_rate: u32,
    toggle_dtr_on_open: bool,
    usb_identity: Option<UsbIdentity>,
    /// Other ports present at the last successful open. A replacement is
    /// never chosen from these: they belong to other devices.
    known_other_ports: HashSet<String>,
    port: Option<Box<dyn serialport::SerialPort>>,
    line_buffer: Vec<u8>,
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
        let mut transport = Self {
            port_name: port_name.to_string(),
            baud_rate,
            toggle_dtr_on_open,
            usb_identity: None,
            known_other_ports: HashSet::new(),
            port: None,
            line_buffer: Vec::new(),
        };
        if let Some(ports) = usb_ports() {
            transport.remember_ports(&ports);
        }
        transport
    }

    /// Record this port's identity and every other port present now.
    fn remember_ports(&mut self, ports: &[UsbPort]) {
        if let Some(port) = ports.iter().find(|port| port.name == self.port_name) {
            self.usb_identity = port.identity.clone().or(self.usb_identity.take());
        }
        self.known_other_ports = ports
            .iter()
            .filter(|port| port.name != self.port_name)
            .map(|port| port.name.clone())
            .collect();
    }

    fn rediscover_usb_port(&mut self) {
        let Some(ports) = usb_ports() else {
            return;
        };
        if ports.iter().any(|port| port.name == self.port_name) {
            return;
        }
        let Some(usb_identity) = self.usb_identity.as_ref() else {
            return;
        };
        let Some(replacement) = replacement_usb_port(
            &self.port_name,
            usb_identity,
            &self.known_other_ports,
            &ports,
        ) else {
            return;
        };
        debug!(old_port = %self.port_name, new_port = %replacement, "USB serial port name changed");
        self.port_name = replacement;
    }
}

fn port_name_family(port_name: &str) -> &str {
    port_name.trim_end_matches(|character: char| character.is_ascii_digit())
}

/// The renamed port of the same physical device, or None when that cannot be
/// established. Ports that already existed beside ours belong to other
/// devices; with a serial number, only an exact match qualifies.
fn replacement_usb_port(
    old_port_name: &str,
    usb_identity: &UsbIdentity,
    known_other_ports: &HashSet<String>,
    ports: &[UsbPort],
) -> Option<String> {
    let matches = ports
        .iter()
        .filter(|port| !known_other_ports.contains(&port.name))
        .filter(|port| {
            port.identity.as_ref().is_some_and(|identity| {
                identity.vid == usb_identity.vid
                    && identity.pid == usb_identity.pid
                    && (usb_identity.serial.is_none() || identity.serial == usb_identity.serial)
            })
        })
        .collect::<Vec<_>>();
    let same_family = matches
        .iter()
        .filter(|port| port_name_family(&port.name) == port_name_family(old_port_name))
        .collect::<Vec<_>>();
    match same_family.as_slice() {
        [port] => Some(port.name.clone()),
        [] if matches.len() == 1 => Some(matches[0].name.clone()),
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

impl RealSerialTransport {
    /// Next complete line. Bytes are decoded per complete line, so a
    /// multi-byte character split across two reads stays intact.
    fn take_buffered_line(&mut self) -> Option<String> {
        let newline = self.line_buffer.iter().position(|byte| *byte == b'\n')?;
        let mut line: Vec<u8> = self.line_buffer.drain(..=newline).collect();
        line.pop();
        if line.last() == Some(&b'\r') {
            line.pop();
        }
        Some(match String::from_utf8(line) {
            Ok(line) => line,
            Err(error) => {
                warn!("Non-UTF8 data received, using lossy conversion");
                String::from_utf8_lossy(error.as_bytes()).into_owned()
            }
        })
    }
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

        self.port = Some(port);
        self.line_buffer.clear();
        if let Some(ports) = usb_ports() {
            self.remember_ports(&ports);
        }

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
        // Hand out a line that already arrived before reading again: a read
        // error must not hide replies received earlier (an error code, an
        // alarm, or the banner that confirms an Emergency Stop).
        if let Some(line) = self.take_buffered_line() {
            return Ok(Some(line));
        }
        let new_bytes = self.read_available()?;
        self.line_buffer.extend_from_slice(&new_bytes);
        if let Some(line) = self.take_buffered_line() {
            return Ok(Some(line));
        }
        if self.line_buffer.len() > MAX_LINE_BUFFER_BYTES {
            let held = self.line_buffer.len();
            self.line_buffer.clear();
            return Err(SerialError::IoError(std::io::Error::other(format!(
                "{} sent {held} bytes without a line ending; check the baud rate",
                self.port_name
            ))));
        }
        Ok(None)
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
    use crate::telemetry::{SERIAL_TRAFFIC_TEST_LOCK, recent_serial_traffic, reset_serial_traffic};

    fn usb_port(port_name: &str, vid: u16, pid: u16) -> UsbPort {
        usb_port_with_serial(port_name, vid, pid, None)
    }

    fn usb_port_with_serial(port_name: &str, vid: u16, pid: u16, serial: Option<&str>) -> UsbPort {
        UsbPort {
            name: port_name.to_string(),
            identity: Some(UsbIdentity {
                vid,
                pid,
                serial: serial.map(str::to_string),
            }),
        }
    }

    fn ch340(serial: Option<&str>) -> UsbIdentity {
        UsbIdentity {
            vid: 0x1a86,
            pid: 0x7523,
            serial: serial.map(str::to_string),
        }
    }

    #[test]
    fn never_switches_to_a_device_that_was_already_connected() {
        // The laser on ttyUSB0 drops; a printer with the same CH340 chip was
        // already on ttyUSB1. Reconnect must not pick the printer.
        let known: HashSet<String> = ["/dev/ttyUSB1".to_string()].into();
        let ports = vec![usb_port("/dev/ttyUSB1", 0x1a86, 0x7523)];
        assert_eq!(replacement_usb_port("/dev/ttyUSB0", &ch340(None), &known, &ports), None);
        // When the laser reappears under a new name, it is found.
        let ports = vec![
            usb_port("/dev/ttyUSB1", 0x1a86, 0x7523),
            usb_port("/dev/ttyUSB2", 0x1a86, 0x7523),
        ];
        assert_eq!(
            replacement_usb_port("/dev/ttyUSB0", &ch340(None), &known, &ports).as_deref(),
            Some("/dev/ttyUSB2")
        );
    }

    #[test]
    fn a_serial_number_must_match_when_the_device_has_one() {
        let known = HashSet::new();
        let other = vec![usb_port_with_serial("COM5", 0x1a86, 0x7523, Some("B"))];
        assert_eq!(replacement_usb_port("COM3", &ch340(Some("A")), &known, &other), None);
        let same = vec![usb_port_with_serial("COM5", 0x1a86, 0x7523, Some("A"))];
        assert_eq!(
            replacement_usb_port("COM3", &ch340(Some("A")), &known, &same).as_deref(),
            Some("COM5")
        );
    }

    #[test]
    fn buffered_replies_survive_a_later_read_error_and_lines_decode_whole() {
        let mut transport = RealSerialTransport::new_without_dtr("unused", 115_200);
        // "é" is two bytes; a read boundary between them must not corrupt it.
        transport.line_buffer.extend_from_slice(b"ok\nALARM:1\n[MSG:\xc3");
        // No port is open, so the next read would fail. Lines already
        // received are still delivered first.
        assert_eq!(transport.read_line().unwrap().as_deref(), Some("ok"));
        assert_eq!(transport.read_line().unwrap().as_deref(), Some("ALARM:1"));
        assert!(transport.read_line().is_err());
        transport.line_buffer.extend_from_slice(b"\xa9]\r\n");
        assert_eq!(transport.take_buffered_line().as_deref(), Some("[MSG:é]"));
    }

    #[test]
    fn rediscovers_a_renamed_usb_port_in_the_same_endpoint_family() {
        let ports = vec![
            usb_port("/dev/cu.usbserial-110", 0x1a86, 0x7523),
            usb_port("/dev/tty.usbserial-110", 0x1a86, 0x7523),
        ];

        let replacement = replacement_usb_port(
            "/dev/cu.usbserial-10",
            &ch340(None),
            &HashSet::new(),
            &ports,
        );

        assert_eq!(replacement.as_deref(), Some("/dev/cu.usbserial-110"));
    }

    #[test]
    fn refuses_an_ambiguous_usb_port_replacement() {
        let ports = vec![
            usb_port("/dev/ttyUSB1", 0x1a86, 0x7523),
            usb_port("/dev/ttyUSB2", 0x1a86, 0x7523),
        ];

        let replacement =
            replacement_usb_port("/dev/ttyUSB0", &ch340(None), &HashSet::new(), &ports);

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

    // scripts/test-macos-serial-open.py also runs this with a simulated driver
    // that returns a stale non-POSIX speed and rejects reapplying it.
    #[cfg(target_os = "macos")]
    #[test]
    fn macos_reopens_virtual_serial_port_and_exchanges_bytes() {
        use serialport::SerialPort;

        let _guard = SERIAL_TRAFFIC_TEST_LOCK.lock().unwrap();
        let (mut master, slave) = serialport::TTYPort::pair().unwrap();
        let port_name = slave.name().unwrap();
        drop(slave);
        // Pseudo terminals do not implement IOSSIOSPEED. Zero skips only that
        // ioctl; the real open/configuration and byte transport still run.
        let mut transport = RealSerialTransport::new_without_dtr(&port_name, 0);
        transport.open().unwrap();

        master.write_all(b"<Idle|MPos:1,2,0>\n").unwrap();
        let deadline = std::time::Instant::now() + Duration::from_secs(1);
        let mut received = Vec::new();
        while !received.ends_with(b"\n") && std::time::Instant::now() < deadline {
            received.extend(transport.read_available().unwrap());
            std::thread::sleep(Duration::from_millis(1));
        }
        assert_eq!(received, b"<Idle|MPos:1,2,0>\n");
        // On macOS tcdrain waits for the pseudo-terminal peer to consume data.
        // Read concurrently with write_line's flush, as a real controller does.
        let reader = std::thread::spawn(move || {
            let mut response = [0; 2];
            master.read_exact(&mut response).unwrap();
            (master, response)
        });
        transport.write_line("?").unwrap();
        let (_master, response) = reader.join().unwrap();
        assert_eq!(&response, b"?\n");
        transport.close().unwrap();
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
    fn portuguese_windows_access_denied_preserves_os_detail_and_port_guidance() {
        let error = serialport::Error::new(serialport::ErrorKind::NoDevice, "Acesso negado.");
        let message = map_open_error_for_platform("COM3", error, true).to_string();
        assert!(message.contains("[serial_port_unavailable]"));
        assert!(message.contains("COM3: Acesso negado."));
        assert!(message.contains("another application"));
        assert!(!message.contains("dialout"));
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
    #[cfg(target_os = "linux")]
    #[test]
    fn linux_real_pty_contention_disconnect_and_reopen() {
        use serialport::SerialPort;
        let _guard = SERIAL_TRAFFIC_TEST_LOCK.lock().unwrap();
        let (mut master, slave) = serialport::TTYPort::pair().unwrap();
        let name = slave.name().unwrap();
        drop(slave);
        let mut first = RealSerialTransport::new_without_dtr(&name, 115_200);
        first.open().unwrap();
        let mut second = RealSerialTransport::new_without_dtr(&name, 115_200);
        assert!(
            second.open().is_err(),
            "a second opener must not steal an exclusive port"
        );
        assert!(!second.is_open());
        first.write_bytes(b"?\n").unwrap();
        let mut query = [0; 2];
        master.read_exact(&mut query).unwrap();
        assert_eq!(&query, b"?\n");
        master.write_all(b"<Idle|MPos:1,2,0>\n").unwrap();
        let deadline = std::time::Instant::now() + Duration::from_secs(2);
        let line = loop {
            if let Some(line) = first.read_line().unwrap() {
                break line;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "status reply timed out"
            );
            std::thread::sleep(Duration::from_millis(1));
        };
        assert_eq!(line, "<Idle|MPos:1,2,0>");
        first.close().unwrap();
        second.open().unwrap();
        master.write_all(b"incomplete-old-session").unwrap();
        // Observe a partial line, then ensure close/open clears it.
        let deadline = std::time::Instant::now() + Duration::from_secs(2);
        while second.line_buffer.is_empty() {
            assert_eq!(second.read_line().unwrap(), None);
            assert!(std::time::Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(1));
        }
        second.close().unwrap();
        second.open().unwrap();
        assert!(second.line_buffer.is_empty());
        drop(master);
        assert!(
            second.read_available().is_err(),
            "a removed PTY must not look idle"
        );
        assert!(matches!(
            second.write_bytes(b"?"),
            Err(SerialError::WriteFailed(_))
        ));
        second.close().unwrap();
        assert!(second.open().is_err());
        assert!(!second.is_open());
    }

    #[cfg(any(target_os = "linux", windows))]
    #[test]
    fn native_access_denied_is_actionable() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("denied-device");
        std::fs::write(&path, b"").unwrap();
        let path = path.canonicalize().unwrap();
        let original = std::fs::metadata(&path).unwrap().permissions();
        let mut denied = original.clone();
        #[cfg(windows)]
        denied.set_readonly(true);
        #[cfg(target_os = "linux")]
        {
            use std::os::unix::fs::{MetadataExt, PermissionsExt};
            assert_ne!(
                std::fs::metadata(&path).unwrap().uid(),
                0,
                "run native serial tests as an unprivileged user"
            );
            denied.set_mode(0);
        }
        std::fs::set_permissions(&path, denied).unwrap();
        let mut transport = RealSerialTransport::new_without_dtr(path.to_str().unwrap(), 115_200);
        let result = transport.open();
        std::fs::set_permissions(&path, original).unwrap();
        let error = result.unwrap_err();
        if cfg!(windows) {
            assert!(
                matches!(error, SerialError::PortUnavailable { .. }),
                "{error}"
            );
        } else {
            assert!(matches!(error, SerialError::AccessDenied { .. }), "{error}");
        }
        assert!(!transport.is_open());
    }

    #[cfg(windows)]
    #[test]
    fn windows_native_invalid_device_and_failed_handle_io() {
        use std::os::windows::io::{FromRawHandle, IntoRawHandle};
        let dir = tempfile::tempdir().unwrap();
        let missing = dir.path().canonicalize().unwrap().join("missing-device");
        let mut transport =
            RealSerialTransport::new_without_dtr(missing.to_str().unwrap(), 115_200);
        assert!(matches!(
            transport.open(),
            Err(SerialError::PortUnavailable { .. })
        ));
        let path = dir.path().join("read-only-handle");
        std::fs::write(&path, b"").unwrap();
        let file = std::fs::File::open(&path).unwrap();
        // A real Windows handle that rejects serial ioctls and writes exercises
        // ClearCommError/WriteFile error propagation without inventing OS text
        // or installing a virtual COM driver on the runner.
        let port = unsafe { serialport::COMPort::from_raw_handle(file.into_raw_handle()) };
        transport.port = Some(Box::new(port));
        assert!(transport.read_available().is_err());
        assert!(matches!(
            transport.write_bytes(b"?"),
            Err(SerialError::WriteFailed(_))
        ));
        transport.close().unwrap();
        assert!(!transport.is_open());
    }
}
