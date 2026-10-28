//! Test-only Win32 boundary for the documented CH340 9600-first failure.
//! Other devices and all tests without an installed scenario use the real APIs.
#![allow(non_snake_case)]

use std::{cell::RefCell, collections::VecDeque, ptr};
use windows_sys::Win32::{
    Devices::Communication::{self as comm, COMMTIMEOUTS, DCB},
    Foundation::{GetHandleInformation, SetLastError, BOOL, ERROR_GEN_FAILURE, HANDLE},
    Security::SECURITY_ATTRIBUTES,
    Storage::FileSystem as fs,
    System::{Threading::CreateEventW, IO::OVERLAPPED},
};

use crate::{Result, SerialPortInfo, SerialPortType, UsbPortInfo};

pub const PORT: &str = "COM_BEAMBENCH_CH340";

#[derive(Clone, Copy)]
pub enum Fault {
    Healthy,
    Needs9600,
    RejectAll(u32),
    BootstrapFails,
    TargetStillFails,
    ReadbackMismatch,
    ReadbackFlagsMismatch,
    ReadbackFails,
    InitialReadFails,
    OpenFails,
    TimeoutsFail,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Settings {
    pub baud: u32,
    pub flags: u32,
    pub bits: u8,
    pub parity: u8,
    pub stop: u8,
}

impl From<DCB> for Settings {
    fn from(dcb: DCB) -> Self {
        Self {
            baud: dcb.BaudRate,
            flags: dcb._bitfield,
            bits: dcb.ByteSize,
            parity: dcb.Parity,
            stop: dcb.StopBits,
        }
    }
}

struct Driver {
    fault: Fault,
    identity: Option<(u16, u16)>,
    handle: HANDLE,
    current: DCB,
    settings: Vec<Settings>,
    enumeration_count: usize,
    verified: bool,
    tx: Vec<u8>,
    rx: VecDeque<u8>,
}

thread_local! {
    static DRIVER: RefCell<Option<Driver>> = const { RefCell::new(None) };
}

pub struct Scenario;

impl Scenario {
    pub fn install(fault: Fault, identity: Option<(u16, u16)>) -> Self {
        DRIVER.with(|slot| {
            assert!(slot.borrow().is_none());
            let mut current: DCB = unsafe { std::mem::zeroed() };
            current.DCBlength = std::mem::size_of::<DCB>() as u32;
            current.BaudRate = 9600;
            *slot.borrow_mut() = Some(Driver {
                fault,
                identity,
                handle: 0,
                current,
                settings: Vec::new(),
                enumeration_count: 0,
                verified: false,
                tx: Vec::new(),
                rx: VecDeque::new(),
            });
        });
        Self
    }

    pub fn settings(&self) -> Vec<Settings> {
        DRIVER.with(|slot| slot.borrow().as_ref().unwrap().settings.clone())
    }

    pub fn enumeration_count(&self) -> usize {
        DRIVER.with(|slot| slot.borrow().as_ref().unwrap().enumeration_count)
    }

    pub fn assert_no_bytes(&self) {
        DRIVER.with(|slot| {
            let slot = slot.borrow();
            let driver = slot.as_ref().unwrap();
            assert!(driver.tx.is_empty());
        });
    }

    pub fn assert_handle_closed(&self) {
        DRIVER.with(|slot| {
            let slot = slot.borrow();
            let driver = slot.as_ref().unwrap();
            if driver.handle != 0 {
                let mut flags = 0;
                assert_eq!(
                    unsafe { GetHandleInformation(driver.handle, &mut flags) },
                    0
                );
            }
        });
    }
}

impl Drop for Scenario {
    fn drop(&mut self) {
        DRIVER.with(|slot| {
            slot.borrow_mut().take();
        });
    }
}

fn with_port<T>(handle: HANDLE, apply: impl FnOnce(&mut Driver) -> T) -> Option<T> {
    DRIVER.with(|slot| {
        slot.borrow_mut()
            .as_mut()
            .filter(|driver| driver.handle != 0 && driver.handle == handle)
            .map(apply)
    })
}

pub fn available_ports() -> Result<Vec<SerialPortInfo>> {
    if let Some(ports) = DRIVER.with(|slot| {
        slot.borrow_mut().as_mut().map(|driver| {
            driver.enumeration_count += 1;
            vec![SerialPortInfo {
                port_name: PORT.into(),
                port_type: match driver.identity {
                    Some((vid, pid)) => SerialPortType::UsbPort(UsbPortInfo {
                        vid,
                        pid,
                        serial_number: None,
                        manufacturer: None,
                        product: None,
                    }),
                    None => SerialPortType::Unknown,
                },
            }]
        })
    }) {
        return Ok(ports);
    }
    super::enumerate::available_ports()
}

pub unsafe fn CreateFileW(
    name: *const u16,
    access: u32,
    share: u32,
    security: *const SECURITY_ATTRIBUTES,
    disposition: u32,
    attributes: u32,
    template: HANDLE,
) -> HANDLE {
    let mut len = 0;
    while *name.add(len) != 0 {
        len += 1;
    }
    let path = String::from_utf16_lossy(std::slice::from_raw_parts(name, len));
    if path.ends_with(PORT) {
        if let Some(handle) = DRIVER.with(|slot| {
            slot.borrow_mut().as_mut().map(|driver| {
                if matches!(driver.fault, Fault::OpenFails) {
                    SetLastError(ERROR_GEN_FAILURE);
                    return -1;
                }
                // A real kernel handle makes the production Drop cleanup observable.
                driver.handle = CreateEventW(ptr::null(), 0, 0, ptr::null());
                assert_ne!(driver.handle, 0);
                driver.handle
            })
        }) {
            return handle;
        }
    }
    fs::CreateFileW(
        name,
        access,
        share,
        security,
        disposition,
        attributes,
        template,
    )
}

pub unsafe fn GetCommState(handle: HANDLE, output: *mut DCB) -> BOOL {
    if let Some(result) = with_port(handle, |driver| {
        if matches!(driver.fault, Fault::InitialReadFails)
            || (matches!(driver.fault, Fault::ReadbackFails) && !driver.settings.is_empty())
        {
            SetLastError(ERROR_GEN_FAILURE);
            return 0;
        }
        *output = driver.current;
        if driver.settings.len() > 1 {
            driver.verified = true;
            if matches!(driver.fault, Fault::ReadbackMismatch) {
                (*output).BaudRate = 9600;
            }
            if matches!(driver.fault, Fault::ReadbackFlagsMismatch) {
                (*output)._bitfield ^= 1 << 4;
            }
        }
        1
    }) {
        return result;
    }
    comm::GetCommState(handle, output)
}

pub unsafe fn SetCommState(handle: HANDLE, input: *const DCB) -> BOOL {
    if let Some(result) = with_port(handle, |driver| {
        let dcb = *input;
        driver.settings.push(dcb.into());
        let attempt = driver.settings.len();
        let error = match driver.fault {
            Fault::Healthy | Fault::TimeoutsFail => None,
            Fault::RejectAll(code) => Some(code),
            Fault::BootstrapFails if attempt == 2 => Some(ERROR_GEN_FAILURE),
            Fault::TargetStillFails if attempt == 3 => Some(ERROR_GEN_FAILURE),
            _ if attempt == 1 && dcb.BaudRate > 9600 => Some(ERROR_GEN_FAILURE),
            _ => None,
        };
        if let Some(code) = error {
            SetLastError(code);
            return 0;
        }
        driver.current = dcb;
        1
    }) {
        return result;
    }
    comm::SetCommState(handle, input)
}

pub unsafe fn SetCommTimeouts(handle: HANDLE, input: *const COMMTIMEOUTS) -> BOOL {
    if let Some(result) = with_port(handle, |driver| {
        if matches!(driver.fault, Fault::TimeoutsFail) {
            SetLastError(ERROR_GEN_FAILURE);
            return 0;
        }
        assert!(!driver.settings.is_empty());
        1
    }) {
        return result;
    }
    comm::SetCommTimeouts(handle, input)
}

pub unsafe fn WriteFile(
    handle: HANDLE,
    buffer: *const u8,
    size: u32,
    written: *mut u32,
    overlapped: *mut OVERLAPPED,
) -> BOOL {
    if let Some(result) = with_port(handle, |driver| {
        assert_eq!(driver.current.BaudRate, 115_200);
        assert!(driver.verified, "no I/O before recovery verification");
        let bytes = std::slice::from_raw_parts(buffer, size as usize);
        driver.tx.extend_from_slice(bytes);
        if bytes == b"?" {
            driver.rx.extend(b"<Idle|MPos:1,2,0|FS:0,0>\r\n");
        }
        *written = size;
        1
    }) {
        return result;
    }
    fs::WriteFile(handle, buffer, size, written, overlapped)
}

pub unsafe fn ReadFile(
    handle: HANDLE,
    buffer: *mut u8,
    size: u32,
    read: *mut u32,
    overlapped: *mut OVERLAPPED,
) -> BOOL {
    if let Some(result) = with_port(handle, |driver| {
        let count = (size as usize).min(driver.rx.len());
        for index in 0..count {
            *buffer.add(index) = driver.rx.pop_front().unwrap();
        }
        *read = count as u32;
        1
    }) {
        return result;
    }
    fs::ReadFile(handle, buffer, size, read, overlapped)
}

pub unsafe fn FlushFileBuffers(handle: HANDLE) -> BOOL {
    if with_port(handle, |_| ()).is_some() {
        return 1;
    }
    fs::FlushFileBuffers(handle)
}
