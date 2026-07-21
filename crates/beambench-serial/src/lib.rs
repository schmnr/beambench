//! Serial port transport for Beam Bench.
//! Handles cross-platform serial communication with laser controllers.

pub mod error;
pub mod mock;
pub mod network;
pub mod port_list;
pub mod real;
pub mod telemetry;
pub mod transport;

pub use error::SerialError;
pub use mock::{MockSerialHandle, MockSerialTransport};
pub use network::{TcpLineTransport, TcpLineTransportConfig};
pub use port_list::list_available_ports;
pub use real::RealSerialTransport;
pub use telemetry::{recent_serial_traffic, record_rx, record_tx, reset_serial_traffic};
pub use transport::SerialTransport;
