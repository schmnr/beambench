//! Serial transport error types.

use thiserror::Error;

#[derive(Debug, Error)]
pub enum SerialError {
    #[error("port not found: {0}")]
    PortNotFound(String),

    #[error("connection failed: {0}")]
    ConnectionFailed(String),

    #[error(
        "[serial_port_unavailable] Could not open {port_name}: {detail}. The port may be in use by another application or the controller may have been disconnected. Close other laser or serial software, reconnect the controller, and try again."
    )]
    PortUnavailable { port_name: String, detail: String },

    // Same OS error code, different likely cause per platform: on Windows an
    // access-denied COM open almost always means another program holds the
    // port; on Linux/macOS EACCES is almost always a device-permission
    // problem (dialout group, udev rules) — port contention there surfaces
    // as busy, not permission denied.
    #[cfg_attr(
        windows,
        error(
            "access denied opening {port_name}: {detail}. Another application may already be using this serial port. Close other laser or serial software, unplug and reconnect the controller, then try again."
        )
    )]
    #[cfg_attr(
        not(windows),
        error(
            "access denied opening {port_name}: {detail}. Your user account may not have permission to open serial devices. On Linux, add your user to the dialout group (or your distro's serial group), then log out and back in."
        )
    )]
    AccessDenied { port_name: String, detail: String },

    #[error("I/O error: {0}")]
    IoError(#[from] std::io::Error),

    #[error("port already open")]
    AlreadyOpen,

    #[error("port not open")]
    NotOpen,

    #[error("read timeout")]
    Timeout,

    #[error("write failed: {0}")]
    WriteFailed(String),
}
