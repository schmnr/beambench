use std::{fmt, io, time::Duration};

use nusb::{
    Device, DeviceInfo, Endpoint, Interface, MaybeFuture,
    descriptors::TransferType,
    transfer::{Buffer, Bulk, ControlOut, ControlType, In, Out, Recipient, TransferError},
};
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::{
    BULK_READ_ENDPOINT, BULK_WRITE_ENDPOINT, USB_PRODUCT_ID, USB_VENDOR_ID, transport::LihuiyuUsbIo,
};

const CH341_PARALLEL_INIT: u8 = 0xb1;
const CH341_EPP_1_9_VALUE: u16 = 0x0102;

/// Read-only information about a CH341 device that could host a supported
/// Lihuiyu controller. A recognized controller status is still required after
/// opening; the USB descriptor alone is not positive controller identity.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LihuiyuUsbDeviceInfo {
    pub bus_id: String,
    pub device_address: u8,
    pub port_numbers: Vec<u8>,
    pub vendor_id: u16,
    pub product_id: u16,
    pub manufacturer: Option<String>,
    pub product: Option<String>,
    pub serial_number: Option<String>,
    /// `None` means the device could be listed but could not be opened for a
    /// descriptor-only endpoint inspection, commonly because of permissions
    /// or the active Windows driver.
    pub has_required_bulk_endpoints: Option<bool>,
    /// Populated on Windows when the operating system reports the active USB
    /// driver. Other platforms leave it unset.
    pub driver: Option<String>,
}

impl LihuiyuUsbDeviceInfo {
    pub fn selector(&self) -> LihuiyuUsbSelector {
        LihuiyuUsbSelector {
            bus_id: self.bus_id.clone(),
            device_address: self.device_address,
            port_numbers: self.port_numbers.clone(),
        }
    }

    pub fn stable_id(&self) -> String {
        self.selector().to_string()
    }
}

/// Selects one physical USB path. The bus and port chain are preferred because
/// a USB address can change after reconnect; address is the fallback on hosts
/// where the operating system cannot report a port chain.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct LihuiyuUsbSelector {
    pub bus_id: String,
    pub device_address: u8,
    #[serde(default)]
    pub port_numbers: Vec<u8>,
}

impl LihuiyuUsbSelector {
    pub fn matches(&self, info: &LihuiyuUsbDeviceInfo) -> bool {
        if self.bus_id != info.bus_id {
            return false;
        }
        if self.port_numbers.is_empty() {
            self.device_address == info.device_address
        } else {
            self.port_numbers == info.port_numbers
        }
    }

    fn matches_native(&self, info: &DeviceInfo) -> bool {
        if self.bus_id != native_bus_id(info) {
            return false;
        }
        if self.port_numbers.is_empty() {
            self.device_address == native_device_address(info)
        } else {
            self.port_numbers == native_port_numbers(info)
        }
    }
}

impl fmt::Display for LihuiyuUsbSelector {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.port_numbers.is_empty() {
            write!(
                formatter,
                "usb-bus-{}-address-{}",
                self.bus_id, self.device_address
            )
        } else {
            let ports = self
                .port_numbers
                .iter()
                .map(u8::to_string)
                .collect::<Vec<_>>()
                .join(".");
            write!(formatter, "usb-bus-{}-ports-{ports}", self.bus_id)
        }
    }
}

#[derive(Debug, Error)]
pub enum LihuiyuUsbError {
    #[error("USB device enumeration failed: {0}")]
    Enumeration(#[source] nusb::Error),
    #[error("no Lihuiyu CH341 device matched {0}")]
    DeviceNotFound(LihuiyuUsbSelector),
    #[error("{count} Lihuiyu CH341 devices matched {selector}; choose one physical USB path")]
    AmbiguousDevice {
        selector: LihuiyuUsbSelector,
        count: usize,
    },
    #[error("opening the CH341 device at {selector} failed: {source}")]
    Open {
        selector: LihuiyuUsbSelector,
        #[source]
        source: nusb::Error,
    },
    #[error("the CH341 device at {0} does not expose bulk endpoints 0x02 and 0x82 together")]
    RequiredBulkInterfaceMissing(LihuiyuUsbSelector),
    #[error("the CH341 device at {selector} exposes {count} possible Lihuiyu bulk interfaces")]
    AmbiguousBulkInterface {
        selector: LihuiyuUsbSelector,
        count: usize,
    },
    #[error("configuring the CH341 device at {selector} failed: {source}")]
    Configure {
        selector: LihuiyuUsbSelector,
        #[source]
        source: nusb::Error,
    },
    #[error("claiming CH341 interface {interface_number} at {selector} failed: {source}")]
    ClaimInterface {
        selector: LihuiyuUsbSelector,
        interface_number: u8,
        #[source]
        source: nusb::Error,
    },
    #[error(
        "selecting CH341 interface {interface_number} alternate setting {alternate_setting} at {selector} failed: {source}"
    )]
    AlternateSetting {
        selector: LihuiyuUsbSelector,
        interface_number: u8,
        alternate_setting: u8,
        #[source]
        source: nusb::Error,
    },
    #[error("opening CH341 bulk endpoint {endpoint:#04x} at {selector} failed: {source}")]
    Endpoint {
        selector: LihuiyuUsbSelector,
        endpoint: u8,
        #[source]
        source: nusb::Error,
    },
    #[error("CH341 read endpoint {endpoint:#04x} at {selector} has a zero packet size")]
    InvalidReadPacketSize {
        selector: LihuiyuUsbSelector,
        endpoint: u8,
    },
}

/// The real native-platform CH341 transport. Construction claims the exact
/// bulk interface but emits no Lihuiyu command; `LihuiyuTransport::connect`
/// owns EPP initialization followed by the recognized-status identity check.
#[derive(Debug)]
pub struct NativeLihuiyuUsbIo {
    interface: Interface,
    write_endpoint: Endpoint<Bulk, Out>,
    read_endpoint: Endpoint<Bulk, In>,
    device_info: LihuiyuUsbDeviceInfo,
}

impl NativeLihuiyuUsbIo {
    pub fn open(selector: &LihuiyuUsbSelector) -> Result<Self, LihuiyuUsbError> {
        let listed = nusb::list_devices()
            .wait()
            .map_err(LihuiyuUsbError::Enumeration)?;
        let mut matches = listed
            .filter(|info| {
                info.vendor_id() == USB_VENDOR_ID
                    && info.product_id() == USB_PRODUCT_ID
                    && selector.matches_native(info)
            })
            .collect::<Vec<_>>();

        if matches.is_empty() {
            return Err(LihuiyuUsbError::DeviceNotFound(selector.clone()));
        }
        if matches.len() != 1 {
            return Err(LihuiyuUsbError::AmbiguousDevice {
                selector: selector.clone(),
                count: matches.len(),
            });
        }

        let listed_info = matches
            .pop()
            .expect("the single matched CH341 device remains available");
        let device = listed_info
            .open()
            .wait()
            .map_err(|source| LihuiyuUsbError::Open {
                selector: selector.clone(),
                source,
            })?;
        let bulk_interface = select_bulk_interface(&device, selector)?;

        let active_configuration = device
            .active_configuration()
            .map(|configuration| configuration.configuration_value())
            .ok();
        if active_configuration != Some(bulk_interface.configuration_number) {
            device
                .set_configuration(bulk_interface.configuration_number)
                .wait()
                .map_err(|source| LihuiyuUsbError::Configure {
                    selector: selector.clone(),
                    source,
                })?;
        }

        let interface = device
            .detach_and_claim_interface(bulk_interface.interface_number)
            .wait()
            .map_err(|source| LihuiyuUsbError::ClaimInterface {
                selector: selector.clone(),
                interface_number: bulk_interface.interface_number,
                source,
            })?;
        if bulk_interface.alternate_setting != 0 {
            interface
                .set_alt_setting(bulk_interface.alternate_setting)
                .wait()
                .map_err(|source| LihuiyuUsbError::AlternateSetting {
                    selector: selector.clone(),
                    interface_number: bulk_interface.interface_number,
                    alternate_setting: bulk_interface.alternate_setting,
                    source,
                })?;
        }

        let write_endpoint = interface
            .endpoint::<Bulk, Out>(BULK_WRITE_ENDPOINT)
            .map_err(|source| LihuiyuUsbError::Endpoint {
                selector: selector.clone(),
                endpoint: BULK_WRITE_ENDPOINT,
                source,
            })?;
        let read_endpoint =
            interface
                .endpoint::<Bulk, In>(BULK_READ_ENDPOINT)
                .map_err(|source| LihuiyuUsbError::Endpoint {
                    selector: selector.clone(),
                    endpoint: BULK_READ_ENDPOINT,
                    source,
                })?;
        if read_endpoint.max_packet_size() == 0 {
            return Err(LihuiyuUsbError::InvalidReadPacketSize {
                selector: selector.clone(),
                endpoint: BULK_READ_ENDPOINT,
            });
        }

        let device_info = native_device_info(&listed_info, Some(true));
        Ok(Self {
            interface,
            write_endpoint,
            read_endpoint,
            device_info,
        })
    }

    pub const fn device_info(&self) -> &LihuiyuUsbDeviceInfo {
        &self.device_info
    }

    pub fn interface_number(&self) -> u8 {
        self.interface.interface_number()
    }
}

impl LihuiyuUsbIo for NativeLihuiyuUsbIo {
    fn initialize_epp_1_9(&mut self, timeout: Duration) -> io::Result<()> {
        validate_timeout(timeout)?;
        self.interface
            .control_out(
                ControlOut {
                    control_type: ControlType::Vendor,
                    recipient: Recipient::Device,
                    request: CH341_PARALLEL_INIT,
                    value: CH341_EPP_1_9_VALUE,
                    index: 0,
                    data: &[],
                },
                timeout,
            )
            .wait()
            .map_err(|error| map_transfer_error("EPP 1.9 initialization", 0, error))
    }

    fn bulk_write(&mut self, data: &[u8], timeout: Duration) -> io::Result<usize> {
        validate_timeout(timeout)?;
        if data.len() > u32::MAX as usize {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "CH341 USB write exceeds the native transfer limit",
            ));
        }
        let completion = self
            .write_endpoint
            .transfer_blocking(Buffer::from(data), timeout);
        let actual_len = completion.actual_len;
        completion
            .status
            .map_err(|error| map_transfer_error("bulk write", actual_len, error))?;
        Ok(actual_len)
    }

    fn bulk_read(&mut self, maximum: usize, timeout: Duration) -> io::Result<Vec<u8>> {
        validate_timeout(timeout)?;
        if maximum == 0 || maximum > u32::MAX as usize {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "CH341 USB read length must be between 1 and the native transfer limit",
            ));
        }
        let packet_size = self.read_endpoint.max_packet_size();
        let requested = maximum.checked_add(packet_size - 1).ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidInput,
                "CH341 USB read length overflow",
            )
        })? / packet_size
            * packet_size;
        if requested > u32::MAX as usize {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "CH341 USB read length exceeds the native transfer limit",
            ));
        }

        let completion = self
            .read_endpoint
            .transfer_blocking(Buffer::new(requested), timeout);
        let actual_len = completion.actual_len;
        completion
            .status
            .map_err(|error| map_transfer_error("bulk read", actual_len, error))?;
        let mut buffer = completion.buffer.into_vec();
        buffer.truncate(actual_len.min(maximum));
        Ok(buffer)
    }
}

pub fn enumerate_lihuiyu_usb_devices() -> Result<Vec<LihuiyuUsbDeviceInfo>, LihuiyuUsbError> {
    let listed = nusb::list_devices()
        .wait()
        .map_err(LihuiyuUsbError::Enumeration)?;
    let mut found = listed
        .filter(|info| info.vendor_id() == USB_VENDOR_ID && info.product_id() == USB_PRODUCT_ID)
        .map(|info| {
            let endpoint_shape = info
                .open()
                .wait()
                .ok()
                .map(|device| bulk_interfaces(&device).len() == 1);
            native_device_info(&info, endpoint_shape)
        })
        .collect::<Vec<_>>();
    found.sort_by(|left, right| {
        left.bus_id
            .cmp(&right.bus_id)
            .then_with(|| left.port_numbers.cmp(&right.port_numbers))
            .then_with(|| left.device_address.cmp(&right.device_address))
    });
    Ok(found)
}

fn native_device_info(
    info: &DeviceInfo,
    has_required_bulk_endpoints: Option<bool>,
) -> LihuiyuUsbDeviceInfo {
    LihuiyuUsbDeviceInfo {
        bus_id: native_bus_id(info),
        device_address: native_device_address(info),
        port_numbers: native_port_numbers(info),
        vendor_id: info.vendor_id(),
        product_id: info.product_id(),
        manufacturer: info.manufacturer_string().map(str::to_owned),
        product: info.product_string().map(str::to_owned),
        serial_number: info.serial_number().map(str::to_owned),
        has_required_bulk_endpoints,
        driver: native_driver(info),
    }
}

#[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]
fn native_bus_id(info: &DeviceInfo) -> String {
    info.bus_id().to_owned()
}

#[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
fn native_bus_id(_info: &DeviceInfo) -> String {
    "unsupported-platform".to_owned()
}

#[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]
fn native_device_address(info: &DeviceInfo) -> u8 {
    info.device_address()
}

#[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
fn native_device_address(_info: &DeviceInfo) -> u8 {
    0
}

#[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]
fn native_port_numbers(info: &DeviceInfo) -> Vec<u8> {
    info.port_chain().to_vec()
}

#[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
fn native_port_numbers(_info: &DeviceInfo) -> Vec<u8> {
    Vec::new()
}

#[cfg(target_os = "windows")]
fn native_driver(info: &DeviceInfo) -> Option<String> {
    info.driver().map(str::to_owned)
}

#[cfg(not(target_os = "windows"))]
fn native_driver(_info: &DeviceInfo) -> Option<String> {
    None
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct BulkInterface {
    configuration_number: u8,
    interface_number: u8,
    alternate_setting: u8,
}

fn select_bulk_interface(
    device: &Device,
    selector: &LihuiyuUsbSelector,
) -> Result<BulkInterface, LihuiyuUsbError> {
    let interfaces = bulk_interfaces(device);
    match interfaces.as_slice() {
        [] => Err(LihuiyuUsbError::RequiredBulkInterfaceMissing(
            selector.clone(),
        )),
        [interface] => Ok(*interface),
        _ => Err(LihuiyuUsbError::AmbiguousBulkInterface {
            selector: selector.clone(),
            count: interfaces.len(),
        }),
    }
}

fn bulk_interfaces(device: &Device) -> Vec<BulkInterface> {
    let mut matches = Vec::new();
    for configuration in device.configurations() {
        for setting in configuration.interface_alt_settings() {
            if has_required_bulk_endpoints(
                setting
                    .endpoints()
                    .map(|endpoint| (endpoint.address(), endpoint.transfer_type())),
            ) {
                matches.push(BulkInterface {
                    configuration_number: configuration.configuration_value(),
                    interface_number: setting.interface_number(),
                    alternate_setting: setting.alternate_setting(),
                });
            }
        }
    }
    matches
}

fn has_required_bulk_endpoints(endpoints: impl IntoIterator<Item = (u8, TransferType)>) -> bool {
    let mut write_bulk = false;
    let mut read_bulk = false;
    for (address, transfer_type) in endpoints {
        if transfer_type != TransferType::Bulk {
            continue;
        }
        match address {
            BULK_WRITE_ENDPOINT => write_bulk = true,
            BULK_READ_ENDPOINT => read_bulk = true,
            _ => {}
        }
    }
    write_bulk && read_bulk
}

fn validate_timeout(timeout: Duration) -> io::Result<()> {
    if timeout.is_zero() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "CH341 USB timeout must be greater than zero",
        ));
    }
    Ok(())
}

fn map_transfer_error(
    operation: &'static str,
    actual_len: usize,
    error: TransferError,
) -> io::Error {
    let kind = match error {
        TransferError::Cancelled => io::ErrorKind::TimedOut,
        TransferError::Stall => io::ErrorKind::BrokenPipe,
        TransferError::Disconnected => io::ErrorKind::NotConnected,
        TransferError::InvalidArgument => io::ErrorKind::InvalidInput,
        TransferError::Fault | TransferError::Unknown(_) => io::ErrorKind::Other,
    };
    let suffix = if actual_len == 0 {
        String::new()
    } else {
        format!(" after transferring {actual_len} bytes")
    };
    io::Error::new(
        kind,
        format!("CH341 USB {operation} failed{suffix}: {error}"),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn info(bus: &str, address: u8, ports: &[u8]) -> LihuiyuUsbDeviceInfo {
        LihuiyuUsbDeviceInfo {
            bus_id: bus.to_owned(),
            device_address: address,
            port_numbers: ports.to_vec(),
            vendor_id: USB_VENDOR_ID,
            product_id: USB_PRODUCT_ID,
            manufacturer: None,
            product: None,
            serial_number: None,
            has_required_bulk_endpoints: Some(true),
            driver: None,
        }
    }

    #[test]
    fn selector_prefers_physical_port_chain_over_ephemeral_address() {
        let selector = info("3", 7, &[2, 4]).selector();
        assert!(selector.matches(&info("3", 19, &[2, 4])));
        assert!(!selector.matches(&info("3", 7, &[2, 5])));
        assert!(!selector.matches(&info("4", 7, &[2, 4])));
        assert_eq!(selector.to_string(), "usb-bus-3-ports-2.4");
    }

    #[test]
    fn selector_uses_address_when_port_chain_is_unavailable() {
        let selector = info("3", 7, &[]).selector();
        assert!(selector.matches(&info("3", 7, &[])));
        assert!(!selector.matches(&info("3", 8, &[])));
        assert_eq!(selector.to_string(), "usb-bus-3-address-7");
    }

    #[test]
    fn transfer_errors_preserve_actionable_io_kinds_and_partial_delivery() {
        assert_eq!(
            map_transfer_error("read", 0, TransferError::Cancelled).kind(),
            io::ErrorKind::TimedOut
        );
        assert_eq!(
            map_transfer_error("read", 0, TransferError::Disconnected).kind(),
            io::ErrorKind::NotConnected
        );
        let partial = map_transfer_error("write", 17, TransferError::Fault);
        assert_eq!(partial.kind(), io::ErrorKind::Other);
        assert!(partial.to_string().contains("transferring 17 bytes"));
    }

    #[test]
    fn zero_timeout_is_rejected_before_native_usb_can_block_forever() {
        let error = validate_timeout(Duration::ZERO).unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::InvalidInput);
    }

    #[test]
    fn interface_requires_both_documented_bulk_endpoints() {
        assert!(has_required_bulk_endpoints([
            (BULK_WRITE_ENDPOINT, TransferType::Bulk),
            (BULK_READ_ENDPOINT, TransferType::Bulk),
            (0x81, TransferType::Interrupt),
        ]));
        assert!(!has_required_bulk_endpoints([
            (BULK_WRITE_ENDPOINT, TransferType::Bulk),
            (BULK_READ_ENDPOINT, TransferType::Interrupt),
        ]));
        assert!(!has_required_bulk_endpoints([
            (BULK_WRITE_ENDPOINT, TransferType::Bulk),
            (0x81, TransferType::Bulk),
        ]));
    }

    #[test]
    fn epp_initialization_matches_the_pinned_ch341_sequence() {
        assert_eq!(CH341_PARALLEL_INIT, 0xb1);
        assert_eq!(CH341_EPP_1_9_VALUE, 0x0102);
    }
}
