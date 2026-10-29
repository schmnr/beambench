use std::mem::MaybeUninit;
use std::os::windows::prelude::*;
use std::time::Duration;
use std::{io, ptr};

#[cfg(test)]
use super::test_driver::{
    CloseHandle, CreateFileW, FlushFileBuffers, ReadFile, SetCommTimeouts, WriteFile,
};
use windows_sys::Win32::Devices::Communication::*;
#[cfg(not(test))]
use windows_sys::Win32::Foundation::CloseHandle;
use windows_sys::Win32::Foundation::{
    DuplicateHandle, GetLastError, DUPLICATE_SAME_ACCESS, ERROR_FILE_NOT_FOUND,
    ERROR_PATH_NOT_FOUND, GENERIC_READ, GENERIC_WRITE, HANDLE, INVALID_HANDLE_VALUE, TRUE,
};
#[cfg(not(test))]
use windows_sys::Win32::Storage::FileSystem::{CreateFileW, FlushFileBuffers, ReadFile, WriteFile};
use windows_sys::Win32::Storage::FileSystem::{FILE_ATTRIBUTE_NORMAL, OPEN_EXISTING};
use windows_sys::Win32::System::Threading::GetCurrentProcess;

use crate::windows::dcb::{self, DCBBitField};
use crate::{
    ClearBuffer, DataBits, Error, ErrorKind, FlowControl, Parity, Result, SerialPort,
    SerialPortBuilder, StopBits,
};

/// A serial port implementation for Windows COM ports
///
/// The port will be closed when the value is dropped. However, this struct
/// should not be instantiated directly by using `COMPort::open()`, instead use
/// the cross-platform `serialport::open()` or
/// `serialport::open_with_settings()`.
#[derive(Debug)]
pub struct COMPort {
    handle: HANDLE,
    timeout: Duration,
    port_name: Option<String>,
}

unsafe impl Send for COMPort {}

impl COMPort {
    /// Opens a COM port as a serial device.
    ///
    /// `port` should be the name of a COM port, e.g., `COM1`.
    ///
    /// If the COM port handle needs to be opened with special flags, use
    /// `from_raw_handle` method to create the `COMPort`. Note that you should
    /// set the different settings before using the serial port using `set_all`.
    ///
    /// ## Errors
    ///
    /// * `NoDevice` if the device could not be opened. This could indicate that
    ///   the device is already in use.
    /// * `InvalidInput` if `port` is not a valid device name.
    /// * `Io` for any other I/O error while opening or initializing the device.
    pub fn open(builder: &SerialPortBuilder) -> Result<COMPort> {
        let mut name = Vec::<u16>::with_capacity(4 + builder.path.len() + 1);

        if !builder.path.starts_with('\\') {
            name.extend(r"\\.\".encode_utf16());
        }

        name.extend(builder.path.encode_utf16());
        name.push(0);

        let share_mode = 0;

        let open = |name: &[u16]| unsafe {
            CreateFileW(
                name.as_ptr(),
                GENERIC_READ | GENERIC_WRITE,
                share_mode,
                ptr::null_mut(),
                OPEN_EXISTING,
                FILE_ATTRIBUTE_NORMAL,
                0 as HANDLE,
            )
        };
        let mut handle = open(&name);
        if handle == INVALID_HANDLE_VALUE {
            let code = unsafe { GetLastError() };
            let original = super::error::last_os_error_at("CreateFileW");
            if !matches!(code, ERROR_FILE_NOT_FOUND | ERROR_PATH_NOT_FOUND) {
                return Err(original);
            }
            let Some(path) = super::enumerate::interface_for_com_port(&builder.path) else {
                return Err(original);
            };
            let path = path.encode_utf16().chain(Some(0)).collect::<Vec<_>>();
            handle = open(&path);
            if handle == INVALID_HANDLE_VALUE {
                return Err(super::error::last_os_error_at(
                    "CreateFileW (COM device interface)",
                ));
            }
        }

        // create the COMPort here so the handle is getting closed
        // if one of the calls to `get_dcb()` or `set_dcb()` fails
        let mut com = COMPort::open_from_raw_handle(handle as RawHandle);

        let mut dcb = dcb::get_dcb(handle)?;
        dcb::init(&mut dcb, builder.dtr_on_open);
        dcb::set_baud_rate(&mut dcb, builder.baud_rate);
        dcb::set_data_bits(&mut dcb, builder.data_bits);
        dcb::set_parity(&mut dcb, builder.parity);
        dcb::set_stop_bits(&mut dcb, builder.stop_bits);
        dcb::set_flow_control(&mut dcb, builder.flow_control);
        dcb::set_dcb_on_open(handle, dcb, &builder.path)?;

        // Try to set DTR on best-effort.
        if let Some(dtr) = builder.dtr_on_open {
            let _ = com.write_data_terminal_ready(dtr);
        }

        com.set_timeout(builder.timeout)?;
        com.port_name = Some(builder.path.clone());
        Ok(com)
    }

    /// Attempts to clone the `SerialPort`. This allow you to write and read simultaneously from the
    /// same serial connection. Please note that if you want a real asynchronous serial port you
    /// should look at [mio-serial](https://crates.io/crates/mio-serial) or
    /// [tokio-serial](https://crates.io/crates/tokio-serial).
    ///
    /// Also, you must be very careful when changing the settings of a cloned `SerialPort` : since
    /// the settings are cached on a per object basis, trying to modify them from two different
    /// objects can cause some nasty behavior.
    ///
    /// This is the same as `SerialPort::try_clone()` but returns the concrete type instead.
    ///
    /// # Errors
    ///
    /// This function returns an error if the serial port couldn't be cloned.
    pub fn try_clone_native(&self) -> Result<COMPort> {
        let process_handle: HANDLE = unsafe { GetCurrentProcess() };
        let mut cloned_handle: HANDLE = INVALID_HANDLE_VALUE;
        unsafe {
            DuplicateHandle(
                process_handle,
                self.handle,
                process_handle,
                &mut cloned_handle,
                0,
                TRUE,
                DUPLICATE_SAME_ACCESS,
            );
            if cloned_handle != INVALID_HANDLE_VALUE {
                Ok(COMPort {
                    handle: cloned_handle,
                    port_name: self.port_name.clone(),
                    timeout: self.timeout,
                })
            } else {
                Err(super::error::last_os_error())
            }
        }
    }

    fn escape_comm_function(&mut self, function: u32) -> Result<()> {
        match unsafe { EscapeCommFunction(self.handle, function) } {
            0 => Err(super::error::last_os_error()),
            _ => Ok(()),
        }
    }

    fn read_pin(&mut self, pin: u32) -> Result<bool> {
        let mut status: u32 = 0;

        match unsafe { GetCommModemStatus(self.handle, &mut status) } {
            0 => Err(super::error::last_os_error()),
            _ => Ok(status & pin != 0),
        }
    }

    fn open_from_raw_handle(handle: RawHandle) -> Self {
        // It is not trivial to get the file path corresponding to a handle.
        // We'll punt and set it `None` here.
        COMPort {
            handle: handle as HANDLE,
            timeout: Duration::from_millis(100),
            port_name: None,
        }
    }

    fn timeout_constant(duration: Duration) -> u32 {
        let milliseconds = duration.as_millis();
        // In the way we are setting up COMMTIMEOUTS, a timeout_constant of u32::MAX gets rejected.
        // Let's clamp the timeout constant for values of u32::MAX and above. See remarks at
        // https://learn.microsoft.com/en-us/windows/win32/api/winbase/ns-winbase-commtimeouts.
        //
        // This effectively throws away accuracy for really long timeouts but at least preserves a
        // long-ish timeout. But just casting to DWORD would result in presumably unexpected short
        // and non-monotonic timeouts from cutting off the higher bits.
        u128::min(milliseconds, u32::MAX as u128 - 1) as u32
    }
}

impl Drop for COMPort {
    fn drop(&mut self) {
        unsafe {
            CloseHandle(self.handle);
        }
    }
}

impl AsRawHandle for COMPort {
    fn as_raw_handle(&self) -> RawHandle {
        self.handle as RawHandle
    }
}

impl FromRawHandle for COMPort {
    unsafe fn from_raw_handle(handle: RawHandle) -> Self {
        COMPort::open_from_raw_handle(handle)
    }
}

impl IntoRawHandle for COMPort {
    fn into_raw_handle(self) -> RawHandle {
        let Self { handle, .. } = self;

        handle as RawHandle
    }
}

impl io::Read for COMPort {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let mut len: u32 = 0;

        match unsafe {
            ReadFile(
                self.handle,
                buf.as_mut_ptr(),
                buf.len() as u32,
                &mut len,
                ptr::null_mut(),
            )
        } {
            0 => Err(io::Error::last_os_error()),
            _ => {
                if len != 0 {
                    Ok(len as usize)
                } else {
                    Err(io::Error::new(
                        io::ErrorKind::TimedOut,
                        "Operation timed out",
                    ))
                }
            }
        }
    }
}

impl io::Write for COMPort {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let mut len: u32 = 0;

        match unsafe {
            WriteFile(
                self.handle,
                buf.as_ptr(),
                buf.len() as u32,
                &mut len,
                ptr::null_mut(),
            )
        } {
            0 => Err(io::Error::last_os_error()),
            _ => Ok(len as usize),
        }
    }

    fn flush(&mut self) -> io::Result<()> {
        match unsafe { FlushFileBuffers(self.handle) } {
            0 => Err(io::Error::last_os_error()),
            _ => Ok(()),
        }
    }
}

impl SerialPort for COMPort {
    fn name(&self) -> Option<String> {
        self.port_name.clone()
    }

    fn timeout(&self) -> Duration {
        self.timeout
    }

    fn set_timeout(&mut self, timeout: Duration) -> Result<()> {
        let timeout_constant = Self::timeout_constant(timeout);

        let timeouts = COMMTIMEOUTS {
            ReadIntervalTimeout: u32::MAX,
            ReadTotalTimeoutMultiplier: u32::MAX,
            ReadTotalTimeoutConstant: timeout_constant,
            WriteTotalTimeoutMultiplier: 0,
            WriteTotalTimeoutConstant: timeout_constant,
        };

        if unsafe { SetCommTimeouts(self.handle, &timeouts) } == 0 {
            return Err(super::error::last_os_error_at("SetCommTimeouts"));
        }

        self.timeout = timeout;
        Ok(())
    }

    fn write_request_to_send(&mut self, level: bool) -> Result<()> {
        if level {
            self.escape_comm_function(SETRTS)
        } else {
            self.escape_comm_function(CLRRTS)
        }
    }

    fn write_data_terminal_ready(&mut self, level: bool) -> Result<()> {
        if level {
            self.escape_comm_function(SETDTR)
        } else {
            self.escape_comm_function(CLRDTR)
        }
    }

    fn read_clear_to_send(&mut self) -> Result<bool> {
        self.read_pin(MS_CTS_ON)
    }

    fn read_data_set_ready(&mut self) -> Result<bool> {
        self.read_pin(MS_DSR_ON)
    }

    fn read_ring_indicator(&mut self) -> Result<bool> {
        self.read_pin(MS_RING_ON)
    }

    fn read_carrier_detect(&mut self) -> Result<bool> {
        self.read_pin(MS_RLSD_ON)
    }

    fn baud_rate(&self) -> Result<u32> {
        let dcb = dcb::get_dcb(self.handle)?;
        Ok(dcb.BaudRate)
    }

    fn data_bits(&self) -> Result<DataBits> {
        let dcb = dcb::get_dcb(self.handle)?;
        match dcb.ByteSize {
            5 => Ok(DataBits::Five),
            6 => Ok(DataBits::Six),
            7 => Ok(DataBits::Seven),
            8 => Ok(DataBits::Eight),
            _ => Err(Error::new(
                ErrorKind::Unknown,
                "Invalid data bits setting encountered",
            )),
        }
    }

    fn parity(&self) -> Result<Parity> {
        let dcb = dcb::get_dcb(self.handle)?;
        match dcb.Parity {
            ODDPARITY => Ok(Parity::Odd),
            EVENPARITY => Ok(Parity::Even),
            NOPARITY => Ok(Parity::None),
            _ => Err(Error::new(
                ErrorKind::Unknown,
                "Invalid parity bits setting encountered",
            )),
        }
    }

    fn stop_bits(&self) -> Result<StopBits> {
        let dcb = dcb::get_dcb(self.handle)?;
        match dcb.StopBits {
            TWOSTOPBITS => Ok(StopBits::Two),
            ONESTOPBIT => Ok(StopBits::One),
            _ => Err(Error::new(
                ErrorKind::Unknown,
                "Invalid stop bits setting encountered",
            )),
        }
    }

    fn flow_control(&self) -> Result<FlowControl> {
        let dcb = dcb::get_dcb(self.handle)?;
        if dcb.fOutxCtsFlow() || dcb.fRtsControl() != dcb::RtsControl::Disable {
            Ok(FlowControl::Hardware)
        } else if dcb.fOutX() || dcb.fInX() {
            Ok(FlowControl::Software)
        } else {
            Ok(FlowControl::None)
        }
    }

    fn set_baud_rate(&mut self, baud_rate: u32) -> Result<()> {
        let mut dcb = dcb::get_dcb(self.handle)?;
        dcb::set_baud_rate(&mut dcb, baud_rate);
        dcb::set_dcb(self.handle, dcb)
    }

    fn set_data_bits(&mut self, data_bits: DataBits) -> Result<()> {
        let mut dcb = dcb::get_dcb(self.handle)?;
        dcb::set_data_bits(&mut dcb, data_bits);
        dcb::set_dcb(self.handle, dcb)
    }

    fn set_parity(&mut self, parity: Parity) -> Result<()> {
        let mut dcb = dcb::get_dcb(self.handle)?;
        dcb::set_parity(&mut dcb, parity);
        dcb::set_dcb(self.handle, dcb)
    }

    fn set_stop_bits(&mut self, stop_bits: StopBits) -> Result<()> {
        let mut dcb = dcb::get_dcb(self.handle)?;
        dcb::set_stop_bits(&mut dcb, stop_bits);
        dcb::set_dcb(self.handle, dcb)
    }

    fn set_flow_control(&mut self, flow_control: FlowControl) -> Result<()> {
        let mut dcb = dcb::get_dcb(self.handle)?;
        dcb::set_flow_control(&mut dcb, flow_control);
        dcb::set_dcb(self.handle, dcb)
    }

    fn bytes_to_read(&self) -> Result<u32> {
        let mut errors: u32 = 0;
        let mut comstat = MaybeUninit::uninit();

        if unsafe { ClearCommError(self.handle, &mut errors, comstat.as_mut_ptr()) != 0 } {
            unsafe { Ok(comstat.assume_init().cbInQue) }
        } else {
            Err(super::error::last_os_error())
        }
    }

    fn bytes_to_write(&self) -> Result<u32> {
        let mut errors: u32 = 0;
        let mut comstat = MaybeUninit::uninit();

        if unsafe { ClearCommError(self.handle, &mut errors, comstat.as_mut_ptr()) != 0 } {
            unsafe { Ok(comstat.assume_init().cbOutQue) }
        } else {
            Err(super::error::last_os_error())
        }
    }

    fn clear(&self, buffer_to_clear: ClearBuffer) -> Result<()> {
        let buffer_flags = match buffer_to_clear {
            ClearBuffer::Input => PURGE_RXABORT | PURGE_RXCLEAR,
            ClearBuffer::Output => PURGE_TXABORT | PURGE_TXCLEAR,
            ClearBuffer::All => PURGE_RXABORT | PURGE_RXCLEAR | PURGE_TXABORT | PURGE_TXCLEAR,
        };

        if unsafe { PurgeComm(self.handle, buffer_flags) != 0 } {
            Ok(())
        } else {
            Err(super::error::last_os_error())
        }
    }

    fn try_clone(&self) -> Result<Box<dyn SerialPort>> {
        match self.try_clone_native() {
            Ok(p) => Ok(Box::new(p)),
            Err(e) => Err(e),
        }
    }

    fn set_break(&self) -> Result<()> {
        if unsafe { SetCommBreak(self.handle) != 0 } {
            Ok(())
        } else {
            Err(super::error::last_os_error())
        }
    }

    fn clear_break(&self) -> Result<()> {
        if unsafe { ClearCommBreak(self.handle) != 0 } {
            Ok(())
        } else {
            Err(super::error::last_os_error())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::timeout::MONOTONIC_DURATIONS;
    use crate::windows::test_driver::{Fault, Scenario, PORT};

    #[test]
    fn missing_com_alias_recovers_via_exact_live_interface_and_exchanges_status() {
        use crate::windows::test_driver::{interface, INTERFACE_PATH, INTERFACE_PORT};
        use std::io::{Read, Write};
        use windows_sys::Win32::Foundation::{ERROR_FILE_NOT_FOUND, ERROR_PATH_NOT_FOUND};

        for error in [ERROR_FILE_NOT_FOUND, ERROR_PATH_NOT_FOUND] {
            for dtr in [None, Some(false), Some(true)] {
                for flow in [
                    FlowControl::None,
                    FlowControl::Software,
                    FlowControl::Hardware,
                ] {
                    let driver = Scenario::interface_only(
                        error,
                        vec![interface(INTERFACE_PORT, INTERFACE_PATH)],
                    );
                    let discovered = crate::available_ports().unwrap();
                    assert_eq!(discovered, [interface(INTERFACE_PORT, INTERFACE_PATH).0]);
                    let mut builder = crate::new(INTERFACE_PORT, 115_200).flow_control(flow);
                    if let Some(dtr) = dtr {
                        builder = builder.dtr_on_open(dtr);
                    }
                    let mut port = builder.open_native().unwrap();
                    assert_eq!(
                        driver.open_paths(),
                        [format!(r"\\.\{INTERFACE_PORT}"), INTERFACE_PATH.into()]
                    );
                    assert_eq!(port.name().as_deref(), Some(INTERFACE_PORT));
                    assert_eq!(port.baud_rate().unwrap(), 115_200);
                    assert_eq!(port.flow_control().unwrap(), flow);
                    assert_eq!(driver.settings().len(), 1);
                    driver.assert_no_bytes();
                    port.write_all(b"?").unwrap();
                    port.flush().unwrap();
                    let expected = b"<Idle|MPos:1,2,0|FS:0,0>\r\n";
                    let mut response = vec![0; expected.len()];
                    port.read_exact(&mut response).unwrap();
                    assert_eq!(&response, expected);
                    drop(port);
                    driver.assert_handle_closed();
                }
            }
        }

        let driver = Scenario::interface_only(
            ERROR_FILE_NOT_FOUND,
            vec![interface(INTERFACE_PORT, INTERFACE_PATH)],
        );
        driver.require_ch340_bootstrap();
        let mut port = crate::new(INTERFACE_PORT, 115_200).open_native().unwrap();
        assert_eq!(
            driver.settings().iter().map(|s| s.baud).collect::<Vec<_>>(),
            [115_200, 9600, 115_200]
        );
        port.write_all(b"?").unwrap();
        let expected = b"<Idle|MPos:1,2,0|FS:0,0>\r\n";
        let mut response = vec![0; expected.len()];
        port.read_exact(&mut response).unwrap();
        assert_eq!(&response, expected);
        drop(port);
        driver.assert_handle_closed();
    }

    #[test]
    fn interface_recovery_never_selects_another_or_ambiguous_com_device() {
        use crate::windows::test_driver::{interface, INTERFACE_PATH, INTERFACE_PORT};
        use windows_sys::Win32::Foundation::ERROR_FILE_NOT_FOUND;

        for interfaces in [
            vec![],
            vec![interface("COM65533", INTERFACE_PATH)],
            vec![
                interface(INTERFACE_PORT, INTERFACE_PATH),
                interface(INTERFACE_PORT, r"\\?\other-controller"),
            ],
        ] {
            let driver = Scenario::interface_only(ERROR_FILE_NOT_FOUND, interfaces);
            let error = crate::new(INTERFACE_PORT, 115_200)
                .open_native()
                .unwrap_err();
            assert_eq!(error.kind(), ErrorKind::NoDevice);
            assert!(error.description.ends_with("(Windows error 2)"), "{error}");
            assert_eq!(driver.open_paths(), [format!(r"\\.\{INTERFACE_PORT}")]);
            assert!(driver.settings().is_empty());
            driver.assert_no_bytes();
            driver.assert_handle_closed();
        }
    }

    #[test]
    fn interface_recovery_preserves_access_and_native_open_errors() {
        use crate::windows::test_driver::{interface, INTERFACE_PATH, INTERFACE_PORT};
        use windows_sys::Win32::Foundation::{
            ERROR_ACCESS_DENIED, ERROR_FILE_NOT_FOUND, ERROR_GEN_FAILURE, ERROR_INVALID_PARAMETER,
        };

        for code in [
            ERROR_ACCESS_DENIED,
            ERROR_GEN_FAILURE,
            ERROR_INVALID_PARAMETER,
        ] {
            let driver =
                Scenario::interface_only(code, vec![interface(INTERFACE_PORT, INTERFACE_PATH)]);
            let error = crate::new(INTERFACE_PORT, 115_200)
                .open_native()
                .unwrap_err();
            assert!(
                error
                    .description
                    .ends_with(&format!("(Windows error {code})")),
                "{error}"
            );
            assert_eq!(driver.enumeration_count(), 0);
            assert_eq!(driver.open_paths().len(), 1);
            driver.assert_no_bytes();
            driver.assert_handle_closed();
        }
        let driver = Scenario::interface_only(
            ERROR_FILE_NOT_FOUND,
            vec![interface(INTERFACE_PORT, INTERFACE_PATH)],
        );
        driver.fail_interface_open(ERROR_ACCESS_DENIED);
        let error = crate::new(INTERFACE_PORT, 115_200)
            .open_native()
            .unwrap_err();
        assert_eq!(error.kind(), ErrorKind::NoDevice);
        assert!(
            error
                .description
                .starts_with("CreateFileW (COM device interface) failed:"),
            "{error}"
        );
        assert!(error.description.ends_with("(Windows error 5)"), "{error}");
        assert_eq!(driver.open_paths().len(), 2);
        driver.assert_no_bytes();
        driver.assert_handle_closed();
    }

    #[test]
    fn ch340_recovers_high_baud_open_and_exchanges_status_at_requested_settings() {
        use std::io::{Read, Write};

        for dtr in [None, Some(false), Some(true)] {
            for flow in [
                FlowControl::None,
                FlowControl::Software,
                FlowControl::Hardware,
            ] {
                let driver = Scenario::install(Fault::Needs9600, Some((0x1a86, 0x7523)));
                let mut builder = crate::new(PORT, 115_200).flow_control(flow);
                if let Some(level) = dtr {
                    builder = builder.dtr_on_open(level);
                }
                let mut port = builder.open_native().unwrap();
                let settings = driver.settings();
                assert_eq!(
                    settings.iter().map(|s| s.baud).collect::<Vec<_>>(),
                    [115_200, 9600, 115_200]
                );
                for applied in &settings[1..] {
                    let mut normalized = applied.clone();
                    normalized.baud = settings[0].baud;
                    assert_eq!(normalized, settings[0], "recovery must change only baud");
                }
                driver.assert_no_bytes();
                port.write_all(b"?").unwrap();
                port.flush().unwrap();
                let expected = b"<Idle|MPos:1,2,0|FS:0,0>\r\n";
                let mut response = vec![0; expected.len()];
                port.read_exact(&mut response).unwrap();
                assert_eq!(&response, expected);
                assert_eq!(port.baud_rate().unwrap(), 115_200);
                drop(port);
                driver.assert_handle_closed();
            }
        }
    }

    #[test]
    fn ch340_recovery_failures_close_without_exposing_a_low_baud_port() {
        for (fault, rates, detail) in [
            (
                Fault::BootstrapFails,
                vec![115_200, 9600],
                "SetCommState failed",
            ),
            (
                Fault::TargetStillFails,
                vec![115_200, 9600, 115_200],
                "SetCommState failed",
            ),
            (
                Fault::ReadbackMismatch,
                vec![115_200, 9600, 115_200],
                "did not retain",
            ),
            (
                Fault::ReadbackFlagsMismatch,
                vec![115_200, 9600, 115_200],
                "did not retain",
            ),
            (
                Fault::ReadbackFails,
                vec![115_200, 9600, 115_200],
                "GetCommState failed",
            ),
        ] {
            let driver = Scenario::install(fault, Some((0x1a86, 0x7523)));
            let error = crate::new(PORT, 115_200).open_native().unwrap_err();
            assert!(error.description.contains(detail), "{error}");
            assert_eq!(
                driver.settings().iter().map(|s| s.baud).collect::<Vec<_>>(),
                rates
            );
            driver.assert_no_bytes();
            driver.assert_handle_closed();
        }
    }

    #[test]
    fn ch340_recovery_does_not_run_for_other_devices_or_unknown_identity() {
        for identity in [
            None,
            Some((0x0403, 0x6001)),
            Some((0x10c4, 0xea60)),
            Some((0x1a86, 0x5523)),
        ] {
            let driver = Scenario::install(Fault::Needs9600, identity);
            let error = crate::new(PORT, 115_200).open_native().unwrap_err();
            assert!(error.description.ends_with("(Windows error 31)"));
            assert_eq!(
                driver.settings().iter().map(|s| s.baud).collect::<Vec<_>>(),
                [115_200]
            );
            driver.assert_no_bytes();
            driver.assert_handle_closed();
        }
    }

    #[test]
    fn healthy_ch340_and_unrelated_failures_do_not_enter_recovery() {
        use windows_sys::Win32::Foundation::{
            ERROR_ACCESS_DENIED, ERROR_GEN_FAILURE, ERROR_INVALID_PARAMETER,
        };

        let driver = Scenario::install(Fault::Healthy, Some((0x1a86, 0x7523)));
        let port = crate::new(PORT, 115_200).open_native().unwrap();
        assert_eq!(driver.settings().len(), 1);
        assert_eq!(driver.enumeration_count(), 0);
        drop(port);
        driver.assert_handle_closed();
        drop(driver);

        for (fault, baud, rates, detail) in [
            (
                Fault::RejectAll(ERROR_ACCESS_DENIED),
                115_200,
                vec![115_200],
                "SetCommState failed",
            ),
            (
                Fault::RejectAll(ERROR_INVALID_PARAMETER),
                115_200,
                vec![115_200],
                "SetCommState failed",
            ),
            (
                Fault::RejectAll(ERROR_GEN_FAILURE),
                9600,
                vec![9600],
                "SetCommState failed",
            ),
            (
                Fault::InitialReadFails,
                115_200,
                vec![],
                "GetCommState failed",
            ),
            (Fault::OpenFails, 115_200, vec![], "CreateFileW failed"),
            (
                Fault::TimeoutsFail,
                115_200,
                vec![115_200],
                "SetCommTimeouts failed",
            ),
        ] {
            let driver = Scenario::install(fault, Some((0x1a86, 0x7523)));
            let error = crate::new(PORT, baud).open_native().unwrap_err();
            assert!(error.description.contains(detail), "{error}");
            assert_eq!(
                driver.settings().iter().map(|s| s.baud).collect::<Vec<_>>(),
                rates
            );
            assert_eq!(driver.enumeration_count(), 0);
            driver.assert_no_bytes();
            driver.assert_handle_closed();
        }
    }

    #[test]
    fn missing_port_identifies_device_open_without_claiming_configuration() {
        let error =
            COMPort::open(&crate::new(r"\\.\BEAMBENCH_MISSING_COM_PORT", 115_200)).unwrap_err();
        assert_eq!(error.kind(), ErrorKind::NoDevice);
        assert!(error.description.starts_with("CreateFileW failed: "));
        assert!(error.description.ends_with("(Windows error 2)"));
    }

    #[test]
    fn failed_timeouts_identify_the_windows_call() {
        let mut port = COMPort {
            handle: INVALID_HANDLE_VALUE,
            timeout: Duration::ZERO,
            port_name: None,
        };
        let error = port.set_timeout(Duration::from_millis(100)).unwrap_err();
        assert!(error.description.starts_with("SetCommTimeouts failed: "));
        assert!(error.description.ends_with("(Windows error 6)"));
        assert_eq!(port.timeout, Duration::ZERO);
    }

    #[test]
    fn timeout_constant_is_monotonic() {
        let mut last = COMPort::timeout_constant(Duration::ZERO);

        for (i, d) in MONOTONIC_DURATIONS.iter().enumerate() {
            let next = COMPort::timeout_constant(*d);
            assert!(
                next >= last,
                "{next} >= {last} failed for {d:?} at index {i}"
            );
            last = next;
        }
    }

    #[test]
    fn timeout_constant_zero_is_zero() {
        assert_eq!(0, COMPort::timeout_constant(Duration::ZERO));
    }
}
