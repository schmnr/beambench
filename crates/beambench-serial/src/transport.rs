//! Byte/line transport trait used by controller protocol sessions.

use crate::error::SerialError;

/// Historical controller-transport abstraction.
///
/// The name is retained for source compatibility, but implementations include
/// serial ports, TCP line streams, and in-memory test transports. Protocol
/// sessions must not assume that DTR or a baud rate exists.
pub trait SerialTransport: Send {
    /// Open the serial port connection.
    fn open(&mut self) -> Result<(), SerialError>;

    /// Close the serial port connection.
    fn close(&mut self) -> Result<(), SerialError>;

    /// Check if the port is currently open.
    fn is_open(&self) -> bool;

    /// Write raw bytes to the port.
    fn write_bytes(&mut self, data: &[u8]) -> Result<usize, SerialError>;

    /// Write a line (appends \n) to the port.
    fn write_line(&mut self, line: &str) -> Result<(), SerialError>;

    /// Read all currently available bytes from the port.
    fn read_available(&mut self) -> Result<Vec<u8>, SerialError>;

    /// Read a complete line (terminated by \n) from the port.
    /// Returns `None` if no complete line is available yet.
    fn read_line(&mut self) -> Result<Option<String>, SerialError>;

    /// Flush the output buffer.
    fn flush(&mut self) -> Result<(), SerialError>;

    /// Set the DTR (Data Terminal Ready) signal.
    /// Used to reset Arduino-based GRBL controllers on connect.
    /// Default implementation is a no-op (for mock transports).
    fn set_dtr(&mut self, _level: bool) -> Result<(), SerialError> {
        Ok(())
    }

    /// Get a stable display name for the transport endpoint.
    fn port_name(&self) -> &str;
}
