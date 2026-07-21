//! Bounded, nonblocking TCP line transport for network GRBL-family controllers.
//!
//! FluidNC and grblHAL expose their normal sender stream through Telnet. The
//! payload is still newline-delimited GRBL traffic, but a conforming client
//! must tolerate Telnet option negotiation interleaved with controller bytes.

use std::io::{Read, Write};
use std::net::{Shutdown, TcpStream, ToSocketAddrs};
use std::time::{Duration, Instant};

use crate::{SerialError, SerialTransport};

const TELNET_IAC: u8 = 255;
const TELNET_DONT: u8 = 254;
const TELNET_DO: u8 = 253;
const TELNET_WONT: u8 = 252;
const TELNET_WILL: u8 = 251;
const TELNET_SB: u8 = 250;
const TELNET_SE: u8 = 240;

/// Connection and resource limits for one TCP controller stream.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TcpLineTransportConfig {
    pub connect_timeout: Duration,
    pub write_timeout: Duration,
    pub retry_interval: Duration,
    pub max_read_bytes_per_poll: usize,
    /// Cap on undrained decoded bytes. Must comfortably exceed
    /// `max_read_bytes_per_poll` plus one 4096-byte read chunk: a poll may
    /// deliver that much at once and this guard must only catch a runaway
    /// controller, not one healthy burst.
    pub max_line_buffer_bytes: usize,
}

impl Default for TcpLineTransportConfig {
    fn default() -> Self {
        Self {
            connect_timeout: Duration::from_secs(3),
            write_timeout: Duration::from_secs(2),
            retry_interval: Duration::from_millis(2),
            max_read_bytes_per_poll: 64 * 1024,
            max_line_buffer_bytes: 256 * 1024,
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
enum TelnetDecodeState {
    #[default]
    Data,
    Iac,
    Negotiate {
        refusal: u8,
    },
    Subnegotiation,
    SubnegotiationIac,
}

#[derive(Debug, Default)]
struct TelnetDecoder {
    state: TelnetDecodeState,
}

impl TelnetDecoder {
    fn decode(&mut self, bytes: &[u8]) -> (Vec<u8>, Vec<u8>) {
        let mut payload = Vec::with_capacity(bytes.len());
        let mut replies = Vec::new();

        for &byte in bytes {
            match self.state {
                TelnetDecodeState::Data if byte == TELNET_IAC => {
                    self.state = TelnetDecodeState::Iac;
                }
                TelnetDecodeState::Data => payload.push(byte),
                TelnetDecodeState::Iac => match byte {
                    TELNET_IAC => {
                        payload.push(TELNET_IAC);
                        self.state = TelnetDecodeState::Data;
                    }
                    TELNET_DO | TELNET_DONT => {
                        self.state = TelnetDecodeState::Negotiate {
                            refusal: TELNET_WONT,
                        };
                    }
                    TELNET_WILL | TELNET_WONT => {
                        self.state = TelnetDecodeState::Negotiate {
                            refusal: TELNET_DONT,
                        };
                    }
                    TELNET_SB => self.state = TelnetDecodeState::Subnegotiation,
                    _ => self.state = TelnetDecodeState::Data,
                },
                TelnetDecodeState::Negotiate { refusal } => {
                    replies.extend([TELNET_IAC, refusal, byte]);
                    self.state = TelnetDecodeState::Data;
                }
                TelnetDecodeState::Subnegotiation if byte == TELNET_IAC => {
                    self.state = TelnetDecodeState::SubnegotiationIac;
                }
                TelnetDecodeState::Subnegotiation => {}
                TelnetDecodeState::SubnegotiationIac if byte == TELNET_SE => {
                    self.state = TelnetDecodeState::Data;
                }
                TelnetDecodeState::SubnegotiationIac if byte == TELNET_IAC => {
                    self.state = TelnetDecodeState::Subnegotiation;
                }
                TelnetDecodeState::SubnegotiationIac => {
                    self.state = TelnetDecodeState::Subnegotiation;
                }
            }
        }

        (payload, replies)
    }
}

/// Blocking-open, nonblocking-I/O TCP stream with newline buffering and
/// conservative Telnet negotiation refusal.
pub struct TcpLineTransport {
    host: String,
    port: u16,
    endpoint: String,
    config: TcpLineTransportConfig,
    stream: Option<TcpStream>,
    line_buffer: Vec<u8>,
    telnet: TelnetDecoder,
}

impl TcpLineTransport {
    pub fn new(host: impl Into<String>, port: u16) -> Self {
        Self::with_config(host, port, TcpLineTransportConfig::default())
    }

    pub fn with_config(host: impl Into<String>, port: u16, config: TcpLineTransportConfig) -> Self {
        let host = host.into();
        let endpoint = display_endpoint(&host, port);
        Self {
            host,
            port,
            endpoint,
            config,
            stream: None,
            line_buffer: Vec::new(),
            telnet: TelnetDecoder::default(),
        }
    }

    fn write_wire_bytes(&mut self, bytes: &[u8]) -> Result<(), SerialError> {
        let stream = self.stream.as_mut().ok_or(SerialError::NotOpen)?;
        let deadline = Instant::now() + self.config.write_timeout;
        let mut written = 0;
        while written < bytes.len() {
            match stream.write(&bytes[written..]) {
                Ok(0) => {
                    return Err(SerialError::WriteFailed(
                        "TCP controller closed the connection".to_string(),
                    ));
                }
                Ok(count) => written += count,
                Err(error) if error.kind() == std::io::ErrorKind::Interrupted => {}
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    if Instant::now() >= deadline {
                        return Err(SerialError::Timeout);
                    }
                    std::thread::sleep(self.config.retry_interval);
                }
                Err(error) => return Err(SerialError::WriteFailed(error.to_string())),
            }
        }
        Ok(())
    }

    fn escaped_telnet_payload(bytes: &[u8]) -> Vec<u8> {
        let extra = bytes.iter().filter(|&&byte| byte == TELNET_IAC).count();
        if extra == 0 {
            return bytes.to_vec();
        }
        let mut escaped = Vec::with_capacity(bytes.len() + extra);
        for &byte in bytes {
            escaped.push(byte);
            if byte == TELNET_IAC {
                escaped.push(TELNET_IAC);
            }
        }
        escaped
    }

    fn take_buffered_line(&mut self) -> Option<String> {
        let newline = self.line_buffer.iter().position(|&byte| byte == b'\n')?;
        let mut line = self.line_buffer.drain(..=newline).collect::<Vec<_>>();
        line.pop();
        if line.last() == Some(&b'\r') {
            line.pop();
        }
        Some(String::from_utf8_lossy(&line).into_owned())
    }
}

impl SerialTransport for TcpLineTransport {
    fn open(&mut self) -> Result<(), SerialError> {
        if self.stream.is_some() {
            return Err(SerialError::AlreadyOpen);
        }
        let host = self.host.trim();
        if host.is_empty() || self.port == 0 {
            return Err(SerialError::ConnectionFailed(
                "TCP controller host and port are required".to_string(),
            ));
        }

        let addresses = (host, self.port)
            .to_socket_addrs()
            .map_err(|error| SerialError::ConnectionFailed(error.to_string()))?
            .collect::<Vec<_>>();
        if addresses.is_empty() {
            return Err(SerialError::ConnectionFailed(format!(
                "{} did not resolve to a network address",
                self.endpoint
            )));
        }

        let deadline = Instant::now() + self.config.connect_timeout;
        let mut last_error = None;
        for address in addresses {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                break;
            }
            match TcpStream::connect_timeout(&address, remaining) {
                Ok(stream) => {
                    stream.set_nodelay(true).map_err(SerialError::IoError)?;
                    stream.set_nonblocking(true).map_err(SerialError::IoError)?;
                    self.stream = Some(stream);
                    self.line_buffer.clear();
                    self.telnet = TelnetDecoder::default();
                    return Ok(());
                }
                Err(error) => last_error = Some(error),
            }
        }

        Err(SerialError::ConnectionFailed(format!(
            "could not connect to {}: {}",
            self.endpoint,
            last_error
                .map(|error| error.to_string())
                .unwrap_or_else(|| "connection timed out".to_string())
        )))
    }

    fn close(&mut self) -> Result<(), SerialError> {
        let stream = self.stream.take().ok_or(SerialError::NotOpen)?;
        let _ = stream.shutdown(Shutdown::Both);
        self.line_buffer.clear();
        self.telnet = TelnetDecoder::default();
        Ok(())
    }

    fn is_open(&self) -> bool {
        self.stream.is_some()
    }

    fn write_bytes(&mut self, data: &[u8]) -> Result<usize, SerialError> {
        let wire = Self::escaped_telnet_payload(data);
        self.write_wire_bytes(&wire)?;
        Ok(data.len())
    }

    fn write_line(&mut self, line: &str) -> Result<(), SerialError> {
        let mut data = line.as_bytes().to_vec();
        data.push(b'\n');
        self.write_bytes(&data)?;
        self.flush()
    }

    fn read_available(&mut self) -> Result<Vec<u8>, SerialError> {
        let mut wire = Vec::new();
        let mut chunk = [0_u8; 4096];
        loop {
            let stream = self.stream.as_mut().ok_or(SerialError::NotOpen)?;
            match stream.read(&mut chunk) {
                Ok(0) => {
                    if wire.is_empty() {
                        self.stream = None;
                        return Err(SerialError::ConnectionFailed(
                            "TCP controller closed the connection".to_string(),
                        ));
                    }
                    // Drain and decode bytes received before the peer's FIN
                    // before marking the transport closed. In particular, a
                    // Telnet negotiation request can arrive in the same burst
                    // as the final application payload, and TCP still permits
                    // us to send the refusal after the peer closes its write
                    // half. A later empty read observes the completed close.
                    break;
                }
                Ok(count) => {
                    wire.extend_from_slice(&chunk[..count]);
                    // The budget bounds per-poll work, not connection health:
                    // a read may straddle it by up to one chunk. Stop reading
                    // and leave the remainder buffered in the socket for the
                    // next poll instead of failing a healthy connection.
                    if wire.len() >= self.config.max_read_bytes_per_poll {
                        break;
                    }
                }
                Err(error) if error.kind() == std::io::ErrorKind::Interrupted => {}
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => break,
                Err(error) => return Err(SerialError::IoError(error)),
            }
        }

        let (payload, negotiation_reply) = self.telnet.decode(&wire);
        if !negotiation_reply.is_empty() {
            self.write_wire_bytes(&negotiation_reply)?;
        }
        Ok(payload)
    }

    fn read_line(&mut self) -> Result<Option<String>, SerialError> {
        if let Some(line) = self.take_buffered_line() {
            return Ok(Some(line));
        }
        let bytes = self.read_available()?;
        if !bytes.is_empty() {
            self.line_buffer.extend(bytes);
            if self.line_buffer.len() > self.config.max_line_buffer_bytes {
                return Err(SerialError::ConnectionFailed(format!(
                    "TCP controller exceeded the {}-byte line-buffer limit",
                    self.config.max_line_buffer_bytes
                )));
            }
        }

        Ok(self.take_buffered_line())
    }

    fn flush(&mut self) -> Result<(), SerialError> {
        self.stream
            .as_mut()
            .ok_or(SerialError::NotOpen)?
            .flush()
            .map_err(|error| SerialError::WriteFailed(error.to_string()))
    }

    fn port_name(&self) -> &str {
        &self.endpoint
    }
}

fn display_endpoint(host: &str, port: u16) -> String {
    let host = host.trim();
    if host.contains(':') && !(host.starts_with('[') && host.ends_with(']')) {
        format!("[{host}]:{port}")
    } else {
        format!("{host}:{port}")
    }
}

#[cfg(test)]
mod tests {
    use std::net::TcpListener;
    use std::sync::mpsc;

    use super::*;

    fn test_config() -> TcpLineTransportConfig {
        TcpLineTransportConfig {
            connect_timeout: Duration::from_secs(1),
            write_timeout: Duration::from_secs(1),
            retry_interval: Duration::from_millis(1),
            max_read_bytes_per_poll: 1024,
            max_line_buffer_bytes: 16 * 1024,
        }
    }

    #[test]
    fn telnet_decoder_is_incremental_and_refuses_options() {
        let mut decoder = TelnetDecoder::default();
        let (first, first_reply) = decoder.decode(b"Grbl\r\n\xff\xfb");
        assert_eq!(first, b"Grbl\r\n");
        assert!(first_reply.is_empty());

        let (second, second_reply) = decoder.decode(&[1, TELNET_IAC, TELNET_SB, 24, 1]);
        assert!(second.is_empty());
        assert_eq!(second_reply, [TELNET_IAC, TELNET_DONT, 1]);

        let (third, third_reply) = decoder.decode(&[TELNET_IAC, TELNET_SE, b'o', b'k', b'\n']);
        assert_eq!(third, b"ok\n");
        assert!(third_reply.is_empty());
    }

    #[test]
    fn burst_larger_than_per_poll_budget_does_not_kill_the_connection() {
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let address = listener.local_addr().unwrap();
        let (hold_tx, hold_rx) = mpsc::channel::<()>();
        let server = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            // One burst well past the 1024-byte per-poll budget: a startup
            // config dump or a stall's worth of buffered reports.
            let mut burst = Vec::new();
            for index in 0..200 {
                burst.extend_from_slice(format!("report-{index:04}\n").as_bytes());
            }
            stream.write_all(&burst).unwrap();
            // Keep the connection open until the client has read everything.
            let _ = hold_rx.recv_timeout(Duration::from_secs(5));
        });

        let mut transport =
            TcpLineTransport::with_config("127.0.0.1", address.port(), test_config());
        transport.open().unwrap();

        let mut lines = 0;
        let deadline = Instant::now() + Duration::from_secs(3);
        while lines < 200 {
            assert!(Instant::now() < deadline, "read {lines} of 200 lines");
            match transport.read_line() {
                Ok(Some(_)) => lines += 1,
                Ok(None) => std::thread::sleep(Duration::from_millis(1)),
                Err(error) => panic!("healthy burst killed the connection: {error}"),
            }
        }
        let _ = hold_tx.send(());
        server.join().unwrap();
    }

    #[test]
    fn tcp_transport_round_trips_grbl_lines_and_telnet_negotiation() {
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let address = listener.local_addr().unwrap();
        let (tx, rx) = mpsc::channel();
        let server = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut command = [0_u8; 3];
            stream.read_exact(&mut command).unwrap();
            tx.send(command).unwrap();
            stream.write_all(&[TELNET_IAC, TELNET_WILL, 1]).unwrap();
            stream
                .write_all(b"[VER:4.0 FluidNC v4.0.3 (esp32-wifi) :]\r\nok\n")
                .unwrap();
            // Make the payload's FIN deterministic while leaving the read
            // half open so the client can return its Telnet refusal.
            stream.shutdown(Shutdown::Write).unwrap();
            let mut refusal = [0_u8; 3];
            stream.read_exact(&mut refusal).unwrap();
            refusal
        });

        let mut transport =
            TcpLineTransport::with_config("127.0.0.1", address.port(), test_config());
        transport.open().unwrap();
        transport.write_line("$I").unwrap();
        assert_eq!(rx.recv_timeout(Duration::from_secs(1)).unwrap(), *b"$I\n");

        let deadline = Instant::now() + Duration::from_secs(1);
        let mut lines = Vec::new();
        while lines.len() < 2 && Instant::now() < deadline {
            if let Some(line) = transport.read_line().unwrap() {
                lines.push(line);
            } else {
                std::thread::sleep(Duration::from_millis(1));
            }
        }
        assert_eq!(lines, ["[VER:4.0 FluidNC v4.0.3 (esp32-wifi) :]", "ok"]);
        assert_eq!(server.join().unwrap(), [TELNET_IAC, TELNET_DONT, 1]);
        transport.close().unwrap();
    }

    #[test]
    fn endpoint_display_brackets_ipv6() {
        assert_eq!(display_endpoint("fluidnc.local", 23), "fluidnc.local:23");
        assert_eq!(display_endpoint("::1", 23), "[::1]:23");
        assert_eq!(display_endpoint("[::1]", 23), "[::1]:23");
    }

    #[test]
    fn complete_lines_are_drained_when_the_peer_closes_immediately() {
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let address = listener.local_addr().unwrap();
        let server = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            stream.write_all(b"first\nsecond\n").unwrap();
        });

        let mut transport =
            TcpLineTransport::with_config("127.0.0.1", address.port(), test_config());
        transport.open().unwrap();
        server.join().unwrap();

        // Loopback delivery can lag the server's join under load; poll until
        // the burst arrives rather than asserting on the first read.
        let deadline = Instant::now() + Duration::from_secs(2);
        let first = loop {
            match transport.read_line().unwrap() {
                Some(line) => break line,
                None => {
                    assert!(Instant::now() < deadline, "no data before deadline");
                    std::thread::sleep(Duration::from_millis(1));
                }
            }
        };
        assert_eq!(first, "first");
        assert_eq!(transport.read_line().unwrap().as_deref(), Some("second"));
        // The FIN may trail the data; poll until the close is observed.
        let deadline = Instant::now() + Duration::from_secs(2);
        while transport.is_open() {
            assert!(Instant::now() < deadline, "close not observed");
            let _ = transport.read_line();
            std::thread::sleep(Duration::from_millis(1));
        }
    }
}
