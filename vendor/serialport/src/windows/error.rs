use std::io;
use std::ptr;

use windows_sys::Win32::Foundation::{
    GetLastError, ERROR_ACCESS_DENIED, ERROR_FILE_NOT_FOUND, ERROR_PATH_NOT_FOUND,
};
use windows_sys::Win32::System::Diagnostics::Debug::{
    FormatMessageW, FORMAT_MESSAGE_FROM_SYSTEM, FORMAT_MESSAGE_IGNORE_INSERTS,
};

use crate::{Error, ErrorKind};

pub fn last_os_error() -> Error {
    os_error(errno())
}

fn os_error(errno: u32) -> Error {
    let kind = match errno {
        ERROR_FILE_NOT_FOUND | ERROR_PATH_NOT_FOUND | ERROR_ACCESS_DENIED => ErrorKind::NoDevice,
        _ => ErrorKind::Io(io::ErrorKind::Other),
    };

    Error::new(
        kind,
        format!("{} (Windows error {errno})", error_string(errno).trim()),
    )
}

pub fn last_os_error_at(operation: &str) -> Error {
    // Capture GetLastError before formatting or any other Windows call.
    os_error_at(errno(), operation)
}

pub fn os_error_at(errno: u32, operation: &str) -> Error {
    let error = os_error(errno);
    Error::new(error.kind(), format!("{operation} failed: {error}"))
}

// the rest of this module is borrowed from libstd

fn errno() -> u32 {
    unsafe { GetLastError() }
}

fn error_string(errnum: u32) -> String {
    let mut buf = [0u16; 2048];

    unsafe {
        let res = FormatMessageW(
            FORMAT_MESSAGE_FROM_SYSTEM | FORMAT_MESSAGE_IGNORE_INSERTS,
            ptr::null(),
            errnum,
            // https://learn.microsoft.com/en-us/windows/win32/api/winbase/nf-winbase-formatmessagew
            // If zero is passed, FormatMessage searches for a message using LANGIDs in this order:
            // 1. Language neutral
            // 2. Thread LANGID (from the thread's locale)
            // 3. User default LANGID (from the user's default locale)
            // 4. System default LANGID (from the system's default locale)
            // 5. US English
            0,
            buf.as_mut_ptr(),
            buf.len() as u32,
            ptr::null(),
        );
        if res == 0 {
            // Sometimes FormatMessageW can fail e.g. system doesn't like langId,
            let fm_err = GetLastError();
            return format!(
                "OS Error {} (FormatMessageW() returned error {})",
                errnum, fm_err
            );
        }

        let b = buf.iter().position(|&b| b == 0).unwrap_or(buf.len());
        let msg = String::from_utf16(&buf[..b]);
        match msg {
            Ok(msg) => msg,
            Err(..) => format!(
                "OS Error {} (FormatMessageW() returned invalid UTF-16)",
                errnum
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use windows_sys::Win32::Foundation::{SetLastError, ERROR_GEN_FAILURE};

    #[test]
    fn device_failure_retains_the_operation_and_numeric_code() {
        unsafe { SetLastError(ERROR_GEN_FAILURE) };
        let error = last_os_error_at("SetCommState");
        assert_eq!(error.kind(), ErrorKind::Io(io::ErrorKind::Other));
        assert!(error.description.starts_with("SetCommState failed: "));
        assert!(error.description.ends_with("(Windows error 31)"));
    }

    #[test]
    fn unavailable_port_errors_keep_their_locale_independent_kind() {
        for code in [
            ERROR_ACCESS_DENIED,
            ERROR_FILE_NOT_FOUND,
            ERROR_PATH_NOT_FOUND,
        ] {
            unsafe { SetLastError(code) };
            let error = last_os_error_at("CreateFileW");
            assert_eq!(error.kind(), ErrorKind::NoDevice);
            assert!(error
                .description
                .ends_with(&format!("(Windows error {code})")));
        }
    }
}
