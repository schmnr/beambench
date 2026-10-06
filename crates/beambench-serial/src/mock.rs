//! Mock serial transport for testing.

use crate::error::SerialError;
use crate::transport::SerialTransport;
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

/// Mock serial transport that records sent data and plays back queued responses.
pub struct MockSerialTransport {
    name: String,
    open: bool,
    rx_queue: Arc<Mutex<VecDeque<String>>>,
    tx_log: Vec<String>,
    tx_bytes: Arc<Mutex<Vec<Vec<u8>>>>,
    tx_lines: Arc<Mutex<Vec<String>>>,
    fail_byte_writes: Arc<Mutex<usize>>,
    line_buffer: String,
}

/// Handle for enqueueing responses after the transport has been boxed into a
/// session — lets tests interleave responses between ticks.
#[derive(Clone)]
pub struct MockSerialHandle {
    rx_queue: Arc<Mutex<VecDeque<String>>>,
    tx_bytes: Arc<Mutex<Vec<Vec<u8>>>>,
    tx_lines: Arc<Mutex<Vec<String>>>,
    fail_byte_writes: Arc<Mutex<usize>>,
}

impl MockSerialHandle {
    /// Queue a response line that will be returned by `read_line`.
    pub fn enqueue_response(&self, line: &str) {
        self.rx_queue
            .lock()
            .expect("mock rx queue poisoned")
            .push_back(line.to_string());
    }

    /// Every `write_bytes` payload (realtime commands), in order.
    pub fn sent_bytes(&self) -> Vec<Vec<u8>> {
        self.tx_bytes.lock().expect("mock tx log poisoned").clone()
    }

    /// Every `write_line` line, in order.
    pub fn sent_lines(&self) -> Vec<String> {
        self.tx_lines.lock().expect("mock tx log poisoned").clone()
    }

    /// Make the next `count` byte writes fail, as a broken link would.
    pub fn fail_next_byte_writes(&self, count: usize) {
        *self
            .fail_byte_writes
            .lock()
            .expect("mock fail count poisoned") = count;
    }
}

impl MockSerialTransport {
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
            open: false,
            rx_queue: Arc::new(Mutex::new(VecDeque::new())),
            tx_log: Vec::new(),
            tx_bytes: Arc::new(Mutex::new(Vec::new())),
            tx_lines: Arc::new(Mutex::new(Vec::new())),
            fail_byte_writes: Arc::new(Mutex::new(0)),
            line_buffer: String::new(),
        }
    }

    /// Queue a response line that will be returned by `read_line`.
    pub fn enqueue_response(&mut self, line: &str) {
        self.rx_queue
            .lock()
            .expect("mock rx queue poisoned")
            .push_back(line.to_string());
    }

    /// Get a handle that can enqueue responses after this transport is boxed.
    pub fn handle(&self) -> MockSerialHandle {
        MockSerialHandle {
            rx_queue: Arc::clone(&self.rx_queue),
            tx_bytes: Arc::clone(&self.tx_bytes),
            tx_lines: Arc::clone(&self.tx_lines),
            fail_byte_writes: Arc::clone(&self.fail_byte_writes),
        }
    }

    /// Get all lines that were sent via `write_line`.
    pub fn sent_lines(&self) -> &[String] {
        &self.tx_log
    }

    /// Clear the sent log.
    pub fn clear_sent(&mut self) {
        self.tx_log.clear();
    }
}

impl SerialTransport for MockSerialTransport {
    fn open(&mut self) -> Result<(), SerialError> {
        if self.open {
            return Err(SerialError::AlreadyOpen);
        }
        self.open = true;
        self.line_buffer.clear();
        Ok(())
    }

    fn close(&mut self) -> Result<(), SerialError> {
        if !self.open {
            return Err(SerialError::NotOpen);
        }
        self.open = false;
        self.line_buffer.clear();
        Ok(())
    }

    fn is_open(&self) -> bool {
        self.open
    }

    fn write_bytes(&mut self, data: &[u8]) -> Result<usize, SerialError> {
        if !self.open {
            return Err(SerialError::NotOpen);
        }
        let mut failures = self
            .fail_byte_writes
            .lock()
            .expect("mock fail count poisoned");
        if *failures > 0 {
            *failures -= 1;
            return Err(SerialError::WriteFailed(
                "injected byte write failure".into(),
            ));
        }
        self.tx_bytes
            .lock()
            .expect("mock tx log poisoned")
            .push(data.to_vec());
        Ok(data.len())
    }

    fn write_line(&mut self, line: &str) -> Result<(), SerialError> {
        if !self.open {
            return Err(SerialError::NotOpen);
        }
        self.tx_log.push(line.to_string());
        self.tx_lines
            .lock()
            .expect("mock tx log poisoned")
            .push(line.to_string());
        Ok(())
    }

    fn read_available(&mut self) -> Result<Vec<u8>, SerialError> {
        if !self.open {
            return Err(SerialError::NotOpen);
        }
        // Drain all queued responses into a single byte buffer
        let mut data = String::new();
        let mut queue = self.rx_queue.lock().expect("mock rx queue poisoned");
        while let Some(line) = queue.pop_front() {
            data.push_str(&line);
            data.push('\n');
        }
        Ok(data.into_bytes())
    }

    fn read_line(&mut self) -> Result<Option<String>, SerialError> {
        if !self.open {
            return Err(SerialError::NotOpen);
        }

        // First check if we already have a complete line in the buffer
        if let Some(newline_pos) = self.line_buffer.find('\n') {
            let line = self.line_buffer[..newline_pos]
                .trim_end_matches('\r')
                .to_string();
            self.line_buffer = self.line_buffer[newline_pos + 1..].to_string();
            return Ok(Some(line));
        }

        // If not, try to get the next queued response
        let next = self
            .rx_queue
            .lock()
            .expect("mock rx queue poisoned")
            .pop_front();
        if let Some(response) = next {
            // Add to buffer with newline and try to extract
            self.line_buffer.push_str(&response);
            self.line_buffer.push('\n');

            if let Some(newline_pos) = self.line_buffer.find('\n') {
                let line = self.line_buffer[..newline_pos]
                    .trim_end_matches('\r')
                    .to_string();
                self.line_buffer = self.line_buffer[newline_pos + 1..].to_string();
                return Ok(Some(line));
            }
        }

        Ok(None)
    }

    fn flush(&mut self) -> Result<(), SerialError> {
        if !self.open {
            return Err(SerialError::NotOpen);
        }
        Ok(())
    }

    fn port_name(&self) -> &str {
        &self.name
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mock_starts_closed() {
        let mock = MockSerialTransport::new("test");
        assert!(!mock.is_open());
    }

    #[test]
    fn mock_open_close_lifecycle() {
        let mut mock = MockSerialTransport::new("test");
        assert!(mock.open().is_ok());
        assert!(mock.is_open());
        assert!(mock.close().is_ok());
        assert!(!mock.is_open());
    }

    #[test]
    fn mock_double_open_fails() {
        let mut mock = MockSerialTransport::new("test");
        mock.open().unwrap();
        assert!(matches!(mock.open(), Err(SerialError::AlreadyOpen)));
    }

    #[test]
    fn mock_close_when_closed_fails() {
        let mut mock = MockSerialTransport::new("test");
        assert!(matches!(mock.close(), Err(SerialError::NotOpen)));
    }

    #[test]
    fn mock_write_line_records() {
        let mut mock = MockSerialTransport::new("test");
        mock.open().unwrap();
        mock.write_line("G0 X10").unwrap();
        mock.write_line("G1 X20 Y30").unwrap();
        assert_eq!(mock.sent_lines(), &["G0 X10", "G1 X20 Y30"]);
    }

    #[test]
    fn mock_write_when_closed_fails() {
        let mut mock = MockSerialTransport::new("test");
        assert!(matches!(mock.write_line("test"), Err(SerialError::NotOpen)));
    }

    #[test]
    fn mock_read_line_returns_queued_responses() {
        let mut mock = MockSerialTransport::new("test");
        mock.enqueue_response("ok");
        mock.enqueue_response("error:1");
        mock.open().unwrap();
        assert_eq!(mock.read_line().unwrap(), Some("ok".to_string()));
        assert_eq!(mock.read_line().unwrap(), Some("error:1".to_string()));
        assert_eq!(mock.read_line().unwrap(), None);
    }

    #[test]
    fn mock_port_name() {
        let mock = MockSerialTransport::new("/dev/ttyUSB0");
        assert_eq!(mock.port_name(), "/dev/ttyUSB0");
    }
}
