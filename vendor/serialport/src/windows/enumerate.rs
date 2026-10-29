use std::collections::HashSet;
use std::ptr;

use windows_sys::core::GUID;
use windows_sys::Win32::Devices::DeviceAndDriverInstallation::{
    CM_Get_DevNode_Status, CM_Get_Device_IDW, CM_Get_Parent, SetupDiClassGuidsFromNameW,
    SetupDiDestroyDeviceInfoList, SetupDiEnumDeviceInfo, SetupDiEnumDeviceInterfaces,
    SetupDiGetClassDevsW, SetupDiGetDeviceInstanceIdW, SetupDiGetDeviceInterfaceDetailW,
    SetupDiGetDeviceRegistryPropertyW, SetupDiOpenDevRegKey, CR_SUCCESS, DICS_FLAG_GLOBAL,
    DIGCF_DEVICEINTERFACE, DIGCF_PRESENT, DIREG_DEV, HDEVINFO, MAX_DEVICE_ID_LEN,
    SPDRP_FRIENDLYNAME, SPDRP_HARDWAREID, SPDRP_MFG, SPINT_ACTIVE, SP_DEVICE_INTERFACE_DATA,
    SP_DEVICE_INTERFACE_DETAIL_DATA_W, SP_DEVINFO_DATA,
};
use windows_sys::Win32::Foundation::{FALSE, FILETIME, INVALID_HANDLE_VALUE, MAX_PATH};
use windows_sys::Win32::System::Registry::{
    RegCloseKey, RegEnumValueW, RegOpenKeyExW, RegQueryInfoKeyW, RegQueryValueExW, HKEY,
    HKEY_LOCAL_MACHINE, KEY_READ, REG_SZ,
};

use crate::{Error, ErrorKind, Result, SerialPortInfo, SerialPortType, UsbPortInfo};

const CONNECTOR_PUNCTUATION_SELECTION: &[char] = &[':', '_', '\u{ff3f}'];

/// takes normal Rust `str` and outputs a null terminated UTF-16 encoded string
fn as_utf16(utf8: &str) -> Vec<u16> {
    utf8.encode_utf16().chain(Some(0)).collect()
}

/// takes a UTF-16 encoded slice (null termination not required)
/// and converts to a UTF8 Rust string. Trailing null chars are removed
fn from_utf16_lossy_trimmed(utf16: &[u16]) -> String {
    String::from_utf16_lossy(utf16)
        .trim_end_matches(0 as char)
        .to_string()
}

/// According to the MSDN docs, we should use SetupDiGetClassDevs, SetupDiEnumDeviceInfo
/// and SetupDiGetDeviceInstanceId in order to enumerate devices.
/// https://msdn.microsoft.com/en-us/windows/hardware/drivers/install/enumerating-installed-devices
fn get_ports_guids() -> Result<Vec<GUID>> {
    #[cfg(test)]
    if super::test_driver::legacy_discovery_disabled() {
        return Ok(Vec::new());
    }
    // SetupDiGetClassDevs returns the devices associated with a particular class of devices.
    // We want the list of devices which are listed as COM ports (generally those that show up in the
    // Device Manager as "Ports (COM & LPT)" which is otherwise known as the "Ports" class).
    //
    // The list of system defined classes can be found here:
    // https://learn.microsoft.com/en-us/windows-hardware/drivers/install/system-defined-device-setup-classes-available-to-vendors
    let class_names = ["Ports", "Modem"];
    let mut guids: Vec<GUID> = Vec::new();
    for class_name in class_names {
        let class_name_w = as_utf16(class_name);
        let mut num_guids: u32 = 1; // Initially assume that there is only 1 guid per name.
        let class_start_idx = guids.len(); // start idx for this name (for potential resize with multiple guids)

        // first attempt with size == 1, second with the size returned from the first try
        for _ in 0..2 {
            guids.resize(class_start_idx + num_guids as usize, GUID::from_u128(0));
            let guid_buffer = &mut guids[class_start_idx..];
            // Find out how many GUIDs are associated with this class name.  num_guids will tell us how many there actually are.
            let res = unsafe {
                SetupDiClassGuidsFromNameW(
                    class_name_w.as_ptr(),
                    guid_buffer.as_mut_ptr(),
                    guid_buffer.len() as u32,
                    &mut num_guids,
                )
            };
            if res == FALSE {
                return Err(Error::new(
                    ErrorKind::Unknown,
                    "Unable to determine number of Ports GUIDs",
                ));
            }
            let len_cmp = guid_buffer.len().cmp(&(num_guids as usize));
            // under allocated
            if len_cmp == std::cmp::Ordering::Less {
                continue; // retry
            }
            // allocation > required len
            else if len_cmp == std::cmp::Ordering::Greater {
                guids.truncate(class_start_idx + num_guids as usize);
            }
            break; // next name
        }
    }
    Ok(guids)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct HwidMatches<'hwid> {
    vid: &'hwid str,
    pid: &'hwid str,
    serial: Option<&'hwid str>,
    interface: Option<&'hwid str>,
}

impl<'hwid> HwidMatches<'hwid> {
    fn new(hwid: &'hwid str) -> Option<Self> {
        // When we match something, update this so that we are always looking forward
        let mut hwid_tail = hwid;

        // VID_(?P<vid>[[:xdigit:]]{4})
        let vid_start = hwid.find("VID_")?;

        // We won't match for hex characters here. That can be done when parsed.
        let vid = hwid_tail.get(vid_start + 4..vid_start + 8)?;
        hwid_tail = hwid_tail.get(vid_start + 8..)?;

        // [&+]PID_(?P<pid>[[:xdigit:]]{4})
        let pid = if hwid_tail.starts_with("&PID_") || hwid_tail.starts_with("+PID_") {
            // We will let the hex parser fail if there are not hex digits.
            hwid_tail.get(5..9)?
        } else {
            return None;
        };
        hwid_tail = hwid_tail.get(9..)?;

        // (?:[&+]MI_(?P<iid>[[:xdigit:]]{2})){0,1}
        let iid = if hwid_tail.starts_with("&MI_") || hwid_tail.starts_with("+MI_") {
            // We will let the hex parser fail if there are not hex digits.
            let iid = hwid_tail.get(4..6);
            hwid_tail = hwid_tail.get(6..).unwrap_or(hwid_tail);

            iid
        } else {
            None
        };

        // ([\\+](?P<serial>\w+))? with slightly modified check for alphanumeric plus some more
        // hand-picked characters instead of regex word character
        //
        // TODO: Fix returning no serial number at all for devices without one. The previous regex
        // and the code below return the first thing from the intance ID. See issue #203.
        let serial = if hwid_tail.starts_with('\\') || hwid_tail.starts_with('+') {
            hwid_tail.get(1..).and_then(|tail| {
                let index = tail
                    .char_indices()
                    .find(|&(_, char)| {
                        !(char.is_alphanumeric() || CONNECTOR_PUNCTUATION_SELECTION.contains(&char))
                    })
                    .map(|(index, _)| index)
                    .unwrap_or(tail.len());
                tail.get(..index)
            })
        } else {
            None
        };

        Some(Self {
            vid,
            pid,
            serial,
            interface: iid,
        })
    }
}

/// Windows usb port information can be determined by the port's HWID string.
///
/// This function parses the HWID string using regex, and returns the USB port
/// information if the hardware ID can be parsed correctly. The manufacturer
/// and product names cannot be determined from the HWID string, so those are
/// set as None.
///
/// For composite USB devices, the HWID string will be for the interface. In
/// this case, the parent HWID string must be provided so that the correct
/// serial number can be determined.
///
/// Some HWID examples are:
///   - MicroPython pyboard:    USB\VID_F055&PID_9802\385435603432
///   - BlackMagic GDB Server:  USB\VID_1D50&PID_6018&MI_00\6&A694CA9&0&0000
///   - BlackMagic UART port:   USB\VID_1D50&PID_6018&MI_02\6&A694CA9&0&0002
///   - FTDI Serial Adapter:    FTDIBUS\VID_0403+PID_6001+A702TB52A\0000
fn parse_usb_port_info(hardware_id: &str, parent_hardware_id: Option<&str>) -> Option<UsbPortInfo> {
    let mut caps = HwidMatches::new(hardware_id)?;

    let interface = caps.interface.and_then(|m| u8::from_str_radix(m, 16).ok());

    if interface.is_some() {
        // If this is a composite device, we need to parse the parent's HWID to get the correct information.
        caps = HwidMatches::new(parent_hardware_id?)?;
    }

    Some(UsbPortInfo {
        vid: u16::from_str_radix(caps.vid, 16).ok()?,
        pid: u16::from_str_radix(caps.pid, 16).ok()?,
        serial_number: caps.serial.map(str::to_string),
        manufacturer: None,
        product: None,

        #[cfg(feature = "usbportinfo-interface")]
        interface,
    })
}

struct PortDevices {
    /// Handle to a device information set.
    hdi: HDEVINFO,

    /// Index used by iterator.
    dev_idx: u32,
}

impl PortDevices {
    // Creates PortDevices object which represents the set of devices associated with a particular
    // Ports class (given by `guid`).
    pub fn new(guid: &GUID) -> Self {
        PortDevices {
            hdi: unsafe { SetupDiGetClassDevsW(guid, ptr::null(), 0, DIGCF_PRESENT) },
            dev_idx: 0,
        }
    }
}

impl Iterator for PortDevices {
    type Item = PortDevice;

    /// Iterator which returns a PortDevice from the set of PortDevices associated with a
    /// particular PortDevices class (guid).
    fn next(&mut self) -> Option<PortDevice> {
        let mut port_dev = PortDevice {
            hdi: self.hdi,
            devinfo_data: SP_DEVINFO_DATA {
                cbSize: std::mem::size_of::<SP_DEVINFO_DATA>() as u32,
                ClassGuid: GUID::from_u128(0),
                DevInst: 0,
                Reserved: 0,
            },
        };
        let res =
            unsafe { SetupDiEnumDeviceInfo(self.hdi, self.dev_idx, &mut port_dev.devinfo_data) };
        if res == FALSE {
            None
        } else {
            self.dev_idx += 1;
            Some(port_dev)
        }
    }
}

impl Drop for PortDevices {
    fn drop(&mut self) {
        // Release the PortDevices object allocated in the constructor.
        unsafe {
            SetupDiDestroyDeviceInfoList(self.hdi);
        }
    }
}

struct PortDevice {
    /// Handle to a device information set.
    hdi: HDEVINFO,

    /// Information associated with this device.
    pub devinfo_data: SP_DEVINFO_DATA,
}

impl PortDevice {
    /// Retrieves the device instance id string associated with this device's parent.
    /// This is useful for determining the serial number of a composite USB device.
    fn parent_instance_id(&mut self) -> Option<String> {
        let mut result_buf = [0u16; MAX_PATH as usize];
        let mut parent_device_instance_id = 0;

        let res =
            unsafe { CM_Get_Parent(&mut parent_device_instance_id, self.devinfo_data.DevInst, 0) };
        if res == CR_SUCCESS {
            let buffer_len = result_buf.len() - 1;
            let res = unsafe {
                CM_Get_Device_IDW(
                    parent_device_instance_id,
                    result_buf.as_mut_ptr(),
                    buffer_len as u32,
                    0,
                )
            };

            if res == CR_SUCCESS {
                Some(from_utf16_lossy_trimmed(&result_buf))
            } else {
                None
            }
        } else {
            None
        }
    }

    /// Retrieves the device instance id string associated with this device. Some examples of
    /// instance id strings are:
    /// * MicroPython Board:  USB\VID_F055&PID_9802\385435603432
    /// * FTDI USB Adapter:   FTDIBUS\VID_0403+PID_6001+A702TB52A\0000
    /// * Black Magic Probe (Composite device with 2 UARTS):
    ///   * GDB Port:       USB\VID_1D50&PID_6018&MI_00\6&A694CA9&0&0000
    ///   * UART Port:      USB\VID_1D50&PID_6018&MI_02\6&A694CA9&0&0002
    ///
    /// Reference: https://learn.microsoft.com/en-us/windows-hardware/drivers/install/device-instance-ids
    fn instance_id(&mut self) -> Option<String> {
        let mut result_buf = [0u16; MAX_DEVICE_ID_LEN as usize];
        let working_buffer_len = result_buf.len() - 1; // always null terminated
        let mut desired_result_len = 0; // possibly larger than the buffer
        let res = unsafe {
            SetupDiGetDeviceInstanceIdW(
                self.hdi,
                &self.devinfo_data,
                result_buf.as_mut_ptr(),
                working_buffer_len as u32,
                &mut desired_result_len,
            )
        };
        if res == FALSE {
            // Try to retrieve hardware id property.
            self.property(SPDRP_HARDWAREID)
        } else {
            let actual_result_len = working_buffer_len.min(desired_result_len as usize);
            Some(from_utf16_lossy_trimmed(&result_buf[..actual_result_len]))
        }
    }

    /// Retrieves the problem status of this device. For example, `CM_PROB_DISABLED` indicates
    /// the device has been disabled in Device Manager.
    fn problem(&mut self) -> Option<u32> {
        let mut status = 0;
        let mut problem_number = 0;
        let res = unsafe {
            CM_Get_DevNode_Status(
                &mut status,
                &mut problem_number,
                self.devinfo_data.DevInst,
                0,
            )
        };
        if res == CR_SUCCESS {
            Some(problem_number)
        } else {
            None
        }
    }

    // Retrieves the port name (i.e. COM6) associated with this device.
    pub fn name(&mut self) -> String {
        // https://learn.microsoft.com/en-us/windows/win32/api/setupapi/nf-setupapi-setupdiopendevregkey
        let hkey = unsafe {
            SetupDiOpenDevRegKey(
                self.hdi,
                &self.devinfo_data,
                DICS_FLAG_GLOBAL,
                0,
                DIREG_DEV,
                KEY_READ,
            )
        };

        if hkey == INVALID_HANDLE_VALUE {
            // failed to open registry key. Return empty string as the failure case
            return String::new();
        }

        // https://learn.microsoft.com/en-us/windows/win32/api/winreg/nf-winreg-regqueryvalueexw
        let mut port_name_buffer = [0u16; MAX_PATH as usize];
        let buffer_byte_len = 2 * port_name_buffer.len() as u32;
        let mut byte_len = buffer_byte_len;
        let mut value_type = 0;

        let value_name = as_utf16("PortName");
        let err = unsafe {
            RegQueryValueExW(
                hkey,
                value_name.as_ptr(),
                ptr::null_mut(),
                &mut value_type,
                port_name_buffer.as_mut_ptr() as *mut u8,
                &mut byte_len,
            )
        };
        unsafe { RegCloseKey(hkey) };
        if err != 0 {
            // failed to query registry for some reason. Return empty string as the failure case
            return String::new();
        }
        // https://learn.microsoft.com/en-us/windows/win32/sysinfo/registry-value-types
        if value_type != REG_SZ || byte_len % 2 != 0 || byte_len > buffer_byte_len {
            // read something but it wasn't the expected registry type
            return String::new();
        }
        // len of u16 chars, not bytes
        let len = buffer_byte_len as usize / 2;
        let port_name = &port_name_buffer[0..len];

        from_utf16_lossy_trimmed(port_name)
    }

    // Determines the port_type for this device, and if it's a USB port populate the various fields.
    pub fn port_type(&mut self) -> SerialPortType {
        self.instance_id()
            .map(|s| (s, self.parent_instance_id())) // Get parent instance id if it exists.
            .and_then(|(d, p)| parse_usb_port_info(&d, p.as_deref()))
            .map(|mut info: UsbPortInfo| {
                info.manufacturer = self.property(SPDRP_MFG);
                info.product = self.property(SPDRP_FRIENDLYNAME);
                SerialPortType::UsbPort(info)
            })
            .unwrap_or(SerialPortType::Unknown)
    }

    // Retrieves a device property and returns it, if it exists. Returns None if the property
    // doesn't exist.
    fn property(&mut self, property_id: u32) -> Option<String> {
        let mut value_type = 0;
        let mut property_buf = [0u16; MAX_PATH as usize];

        let res = unsafe {
            SetupDiGetDeviceRegistryPropertyW(
                self.hdi,
                &self.devinfo_data,
                property_id,
                &mut value_type,
                property_buf.as_mut_ptr() as *mut u8,
                property_buf.len() as u32,
                ptr::null_mut(),
            )
        };

        if res == FALSE || value_type != REG_SZ {
            return None;
        }

        // Using the unicode version of 'SetupDiGetDeviceRegistryProperty' seems to report the
        // entire mfg registry string. This typically includes some driver information that we should discard.
        // Example string: 'FTDI5.inf,%ftdi%;FTDI'
        from_utf16_lossy_trimmed(&property_buf)
            .split(';')
            .next_back()
            .map(str::to_string)
    }
}

/// Not all COM ports are listed under the "Ports" device class
/// The full list of COM ports is available from the registry at
/// HKEY_LOCAL_MACHINE\HARDWARE\DEVICEMAP\SERIALCOMM
///
/// port of https://learn.microsoft.com/en-us/windows/win32/sysinfo/enumerating-registry-subkeys
fn get_registry_com_ports() -> HashSet<String> {
    #[cfg(test)]
    if super::test_driver::legacy_discovery_disabled() {
        return HashSet::new();
    }
    let mut ports_list = HashSet::new();

    let reg_key = as_utf16("HARDWARE\\DEVICEMAP\\SERIALCOMM");
    let key_ptr = reg_key.as_ptr();
    let mut ports_key: HKEY = 0;

    // SAFETY: ffi, all inputs are correct
    let open_res =
        unsafe { RegOpenKeyExW(HKEY_LOCAL_MACHINE, key_ptr, 0, KEY_READ, &mut ports_key) };
    if open_res == 0 {
        let mut class_name_buff = [0u16; MAX_PATH as usize];
        let mut class_name_size = MAX_PATH;
        let mut sub_key_count = 0;
        let mut largest_sub_key = 0;
        let mut largest_class_string = 0;
        let mut num_key_values = 0;
        let mut longest_value_name = 0;
        let mut longest_value_data = 0;
        let mut size_security_desc = 0;
        let mut last_write_time = FILETIME {
            dwLowDateTime: 0,
            dwHighDateTime: 0,
        };
        // SAFETY: ffi, all inputs are correct
        let query_res = unsafe {
            RegQueryInfoKeyW(
                ports_key,
                class_name_buff.as_mut_ptr(),
                &mut class_name_size,
                ptr::null(),
                &mut sub_key_count,
                &mut largest_sub_key,
                &mut largest_class_string,
                &mut num_key_values,
                &mut longest_value_name,
                &mut longest_value_data,
                &mut size_security_desc,
                &mut last_write_time,
            )
        };
        if query_res == 0 {
            for idx in 0..num_key_values {
                let mut val_name_buff = [0u16; MAX_PATH as usize];
                let mut val_name_size = MAX_PATH;
                let mut value_type = 0;
                let mut val_data = [0u16; MAX_PATH as usize];
                let buffer_byte_len = 2 * val_data.len() as u32; // len doubled
                let mut byte_len = buffer_byte_len;

                // SAFETY: ffi, all inputs are correct
                let res = unsafe {
                    RegEnumValueW(
                        ports_key,
                        idx,
                        val_name_buff.as_mut_ptr(),
                        &mut val_name_size,
                        ptr::null(),
                        &mut value_type,
                        val_data.as_mut_ptr() as *mut u8,
                        &mut byte_len,
                    )
                };
                if res != 0
                    || value_type != REG_SZ // only valid for text values
                    || byte_len % 2 != 0 // out byte len should be a multiple of u16 size
                    || byte_len > buffer_byte_len
                {
                    break;
                }
                // key data is returned as u16
                // SAFETY: data_size is checked and pointer is valid
                let val_data = from_utf16_lossy_trimmed(unsafe {
                    let utf16_len = byte_len / 2; // utf16 len
                    std::slice::from_raw_parts(val_data.as_ptr(), utf16_len as usize)
                });
                ports_list.insert(val_data);
            }
        }
        // SAFETY: ffi, all inputs are correct
        unsafe { RegCloseKey(ports_key) };
    }
    ports_list
}

// Drivers can publish live COM interfaces outside the Ports/Modem setup classes,
// and an interface can still open when its legacy COM alias is missing.
// https://learn.microsoft.com/en-us/windows-hardware/drivers/install/guid-devinterface-comport
pub(super) fn com_port_interfaces() -> Vec<(SerialPortInfo, String)> {
    #[cfg(test)]
    if let Some(interfaces) = super::test_driver::com_port_interfaces() {
        return interfaces;
    }

    let guid = GUID::from_u128(0x86e0d1e0_8089_11d0_9ce4_08003e301f73);
    let devices = PortDevices {
        hdi: unsafe {
            SetupDiGetClassDevsW(&guid, ptr::null(), 0, DIGCF_PRESENT | DIGCF_DEVICEINTERFACE)
        },
        dev_idx: 0,
    };
    if devices.hdi == INVALID_HANDLE_VALUE {
        return Vec::new();
    }
    let mut ports = Vec::new();
    for index in 0.. {
        let mut interface: SP_DEVICE_INTERFACE_DATA = unsafe { std::mem::zeroed() };
        interface.cbSize = std::mem::size_of::<SP_DEVICE_INTERFACE_DATA>() as u32;
        if unsafe {
            SetupDiEnumDeviceInterfaces(devices.hdi, ptr::null(), &guid, index, &mut interface)
        } == FALSE
        {
            break;
        }
        if interface.Flags & SPINT_ACTIVE == 0 {
            continue;
        }

        let mut bytes = 0;
        unsafe {
            SetupDiGetDeviceInterfaceDetailW(
                devices.hdi,
                &interface,
                ptr::null_mut(),
                0,
                &mut bytes,
                ptr::null_mut(),
            );
        }
        let header = std::mem::size_of::<SP_DEVICE_INTERFACE_DETAIL_DATA_W>();
        if (bytes as usize) < header {
            continue;
        }
        // The variable-size structure needs pointer alignment, not Vec<u8> alignment.
        let mut buffer = vec![0usize; (bytes as usize).div_ceil(std::mem::size_of::<usize>())];
        let detail = buffer.as_mut_ptr() as *mut SP_DEVICE_INTERFACE_DETAIL_DATA_W;
        let mut device = PortDevice {
            hdi: devices.hdi,
            devinfo_data: unsafe { std::mem::zeroed() },
        };
        device.devinfo_data.cbSize = std::mem::size_of::<SP_DEVINFO_DATA>() as u32;
        unsafe { (*detail).cbSize = header as u32 };
        if unsafe {
            SetupDiGetDeviceInterfaceDetailW(
                devices.hdi,
                &interface,
                detail,
                bytes,
                ptr::null_mut(),
                &mut device.devinfo_data,
            )
        } == FALSE
            || device.problem() != Some(0)
        {
            continue;
        }
        let path = unsafe {
            let start = std::ptr::addr_of!((*detail).DevicePath) as *const u16;
            let offset = start as usize - detail as usize;
            std::slice::from_raw_parts(start, (bytes as usize - offset) / 2)
        };
        let Some(end) = path.iter().position(|c| *c == 0) else {
            continue;
        };
        let path = String::from_utf16_lossy(&path[..end]);
        if path.is_empty() {
            continue;
        }
        let name = device.name();
        ports.push((
            SerialPortInfo {
                port_name: if name.is_empty() { path.clone() } else { name },
                port_type: device.port_type(),
            },
            path,
        ));
    }
    ports
}

pub(super) fn interface_for_com_port(name: &str) -> Option<String> {
    let name = name.strip_prefix(r"\\.\").unwrap_or(name);
    // Never reinterpret arbitrary paths or choose another COM port.
    if name.len() <= 3
        || !name
            .get(..3)
            .is_some_and(|prefix| prefix.eq_ignore_ascii_case("COM"))
        || !name[3..].bytes().all(|b| b.is_ascii_digit())
    {
        return None;
    }
    let mut matching = com_port_interfaces()
        .into_iter()
        .filter(|(port, _)| port.port_name.eq_ignore_ascii_case(name))
        .map(|(_, path)| path);
    let path = matching.next()?;
    // A conflicting registration is not proof of which physical device was requested.
    matching
        .all(|other| other.eq_ignore_ascii_case(&path))
        .then_some(path)
}

/// List available serial ports on the system.
pub fn available_ports() -> Result<Vec<SerialPortInfo>> {
    let mut ports = Vec::new();
    let guids = get_ports_guids();
    for guid in guids.as_ref().into_iter().flatten() {
        let port_devices = PortDevices::new(guid);
        for mut port_device in port_devices {
            // Ignore nonfunctional devices
            if port_device.problem() != Some(0) {
                continue;
            }

            let port_name = port_device.name();

            debug_assert!(
                port_name.as_bytes().last().map_or(true, |c| *c != b'\0'),
                "port_name has a trailing nul: {:?}",
                port_name
            );

            // This technique also returns parallel ports, so we filter these out.
            if port_name.starts_with("LPT") {
                continue;
            }

            ports.push(SerialPortInfo {
                port_name,
                port_type: port_device.port_type(),
            });
        }
    }
    for (port, _) in com_port_interfaces() {
        if let Some(existing) = ports
            .iter_mut()
            .find(|p| p.port_name.eq_ignore_ascii_case(&port.port_name))
        {
            if matches!(existing.port_type, SerialPortType::Unknown) {
                existing.port_type = port.port_type;
            }
        } else {
            ports.push(port);
        }
    }
    merge_registry_com_ports(&mut ports, get_registry_com_ports());
    if ports.is_empty() {
        // A setup-class lookup failure must not discard working interface/registry ports.
        guids?;
    }
    Ok(ports)
}

fn merge_registry_com_ports(ports: &mut Vec<SerialPortInfo>, mut raw_ports_set: HashSet<String>) {
    // Compare names, not counts: either discovery source can contain unique ports.
    // Keep the richer SetupAPI metadata for ports present in both sources.
    for port in ports.iter() {
        raw_ports_set.remove(&port.port_name);
    }
    for port_name in raw_ports_set {
        ports.push(SerialPortInfo {
            port_name,
            port_type: SerialPortType::Unknown,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use quickcheck_macros::quickcheck;

    #[test]
    fn interface_discovery_finds_a_port_with_no_legacy_registration() {
        use super::super::test_driver::{interface, Scenario, INTERFACE_PATH, INTERFACE_PORT};
        use windows_sys::Win32::Foundation::ERROR_FILE_NOT_FOUND;

        let _driver = Scenario::interface_only(
            ERROR_FILE_NOT_FOUND,
            vec![interface(INTERFACE_PORT, INTERFACE_PATH)],
        );
        assert!(get_ports_guids().unwrap().is_empty());
        assert!(get_registry_com_ports().is_empty());
        assert_eq!(
            available_ports().unwrap(),
            [interface(INTERFACE_PORT, INTERFACE_PATH).0],
            "live interface missing from discovery"
        );
        for name in [
            INTERFACE_PORT.to_string(),
            INTERFACE_PORT.to_lowercase(),
            format!(r"\\.\{INTERFACE_PORT}"),
        ] {
            assert_eq!(
                interface_for_com_port(&name).as_deref(),
                Some(INTERFACE_PATH)
            );
        }
        for name in ["", "COM", "COM3x", "LPT1", "𐀀123", "ＣOM3", INTERFACE_PATH] {
            assert_eq!(interface_for_com_port(name), None);
        }
    }

    #[test]
    fn native_interface_scan_returns_only_nonempty_paths() {
        // Real SetupAPI calls, read-only and without opening or resetting devices.
        for (port, path) in com_port_interfaces() {
            assert!(!port.port_name.is_empty());
            assert!(!path.is_empty());
            assert!(!path.contains('\0'));
        }
    }

    #[test]
    fn registry_merge_uses_names_instead_of_counts() {
        let setup_ports = vec![
            SerialPortInfo {
                port_name: "COM1".into(),
                port_type: SerialPortType::UsbPort(crate::UsbPortInfo {
                    vid: 0x1a86,
                    pid: 0x7523,
                    serial_number: Some("controller".into()),
                    manufacturer: Some("USB serial".into()),
                    product: Some("Controller".into()),
                    #[cfg(feature = "usbportinfo-interface")]
                    interface: None,
                }),
            },
            SerialPortInfo {
                port_name: "COM4".into(),
                port_type: SerialPortType::BluetoothPort,
            },
        ];
        for names in [vec!["COM1", "COM3"], vec!["COM3"], vec!["COM1"], vec![]] {
            let mut ports = setup_ports.clone();
            merge_registry_com_ports(
                &mut ports,
                names.iter().map(|name| (*name).into()).collect(),
            );

            assert_eq!(&ports[..2], setup_ports.as_slice());
            if names.contains(&"COM3") {
                assert_eq!(ports.len(), 3);
                assert_eq!(ports[2].port_name, "COM3");
                assert_eq!(ports[2].port_type, SerialPortType::Unknown);
            } else {
                assert_eq!(ports.len(), 2);
            }
        }
    }

    #[test]
    fn registry_merge_handles_empty_setupapi_discovery() {
        let mut ports = Vec::new();
        merge_registry_com_ports(&mut ports, HashSet::new());
        assert!(ports.is_empty());

        merge_registry_com_ports(&mut ports, ["COM3".into()].into_iter().collect());
        assert_eq!(ports.len(), 1);
        assert_eq!(ports[0].port_name, "COM3");
        assert_eq!(ports[0].port_type, SerialPortType::Unknown);
    }

    #[test]
    fn from_utf16_lossy_trimmed_trimming_empty() {
        assert_eq!("", from_utf16_lossy_trimmed(&[]));
        assert_eq!("", from_utf16_lossy_trimmed(&[0]));
    }

    #[test]
    fn from_utf16_lossy_trimmed_trimming() {
        let test_str = "Testing";
        let wtest_str: Vec<u16> = as_utf16(test_str);
        let wtest_str_trailing = wtest_str
            .iter()
            .copied()
            .chain([0, 0, 0, 0]) // add some null chars
            .collect::<Vec<_>>();
        let and_back = from_utf16_lossy_trimmed(&wtest_str_trailing);

        assert_eq!(test_str, and_back);
    }

    // Check that passing some random data to HwidMatches::new() does not cause a panic.
    #[quickcheck]
    fn quickcheck_hwidmatches_new_does_not_panic_from_random_input(hwid: String) -> bool {
        let _ = HwidMatches::new(&hwid);
        true
    }

    // Corner cases which might not always represent what we want to/should parse. But they at
    // least illustrate how we are parsing device identification strings today.
    #[test]
    fn test_hwidmatches_new_corner_cases() {
        assert!(HwidMatches::new("").is_none());
        assert!(HwidMatches::new("ROOT").is_none());
        assert!(HwidMatches::new("ROOT\\").is_none());
        assert!(HwidMatches::new("USB\\").is_none());
        assert!(HwidMatches::new("USB\\VID_1234").is_none());
        assert!(HwidMatches::new("USB\\PID_1234").is_none());
        assert!(HwidMatches::new("USB\\MI_12").is_none());

        assert_eq!(
            HwidMatches::new("VID_1234&PID_5678").unwrap(),
            HwidMatches {
                vid: "1234",
                pid: "5678",
                serial: None,
                interface: None,
            }
        );

        assert_eq!(
            HwidMatches::new("ABC\\VID_1234&PID_5678&MI_90").unwrap(),
            HwidMatches {
                vid: "1234",
                pid: "5678",
                serial: None,
                interface: Some("90"),
            }
        );

        assert_eq!(
            HwidMatches::new("FTDIBUS\\VID_1234&PID_5678&MI_90").unwrap(),
            HwidMatches {
                vid: "1234",
                pid: "5678",
                serial: None,
                interface: Some("90"),
            }
        );

        assert_eq!(
            HwidMatches::new("USB\\VID_1234+PID_5678+MI_90").unwrap(),
            HwidMatches {
                vid: "1234",
                pid: "5678",
                serial: None,
                interface: Some("90"),
            }
        );

        assert_eq!(
            HwidMatches::new("FTDIBUS\\VID_1234+PID_5678\\0000").unwrap(),
            HwidMatches {
                vid: "1234",
                pid: "5678",
                serial: Some("0000"),
                interface: None,
            }
        );
    }

    #[test]
    fn test_hwidmatches_new_standard_cases_ftdi() {
        assert_eq!(
            HwidMatches::new("FTDIBUS\\VID_1234+PID_5678+SERIAL123\\0000").unwrap(),
            HwidMatches {
                vid: "1234",
                pid: "5678",
                serial: Some("SERIAL123"),
                interface: None,
            }
        );
    }

    #[test]
    fn test_hwidmatches_new_standard_cases_usb() {
        assert_eq!(
            HwidMatches::new("USB\\VID_1234&PID_5678").unwrap(),
            HwidMatches {
                vid: "1234",
                pid: "5678",
                serial: None,
                interface: None,
            }
        );

        assert_eq!(
            HwidMatches::new("USB\\VID_1234&PID_5678&MI_90").unwrap(),
            HwidMatches {
                vid: "1234",
                pid: "5678",
                serial: None,
                interface: Some("90"),
            }
        );

        assert_eq!(
            HwidMatches::new("USB\\VID_1234&PID_5678\\SERIAL123").unwrap(),
            HwidMatches {
                vid: "1234",
                pid: "5678",
                serial: Some("SERIAL123"),
                interface: None,
            }
        );

        assert_eq!(
            HwidMatches::new("USB\\VID_1234&PID_5678&MI_90\\SERIAL123").unwrap(),
            HwidMatches {
                vid: "1234",
                pid: "5678",
                serial: Some("SERIAL123"),
                interface: Some("90"),
            }
        );

        // There are ESP32 Arduino devices using colons in their serial numbers. See issue #279.
        assert_eq!(
            HwidMatches::new("USB\\VID_303A&PID_1001\\B4:3A:45:B0:08:24").unwrap(),
            HwidMatches {
                vid: "303A",
                pid: "1001",
                serial: Some("B4:3A:45:B0:08:24"),
                interface: None,
            },
        )
    }

    #[test]
    fn test_parsing_usb_port_information() {
        let madeup_hwid = r"USB\VID_1D50&PID_6018+6&A694CA9&0&0000";
        let info = parse_usb_port_info(madeup_hwid, None).unwrap();
        // TODO: Fix returning no serial at all for devices without one. See issue #203.
        assert_eq!(info.serial_number, Some("6".to_string()));

        let bm_uart_hwid = r"USB\VID_1D50&PID_6018&MI_02\6&A694CA9&0&0000";
        let bm_parent_hwid = r"USB\VID_1D50&PID_6018\85A12F01";
        let info = parse_usb_port_info(bm_uart_hwid, Some(bm_parent_hwid)).unwrap();

        assert_eq!(info.vid, 0x1D50);
        assert_eq!(info.pid, 0x6018);
        assert_eq!(info.serial_number, Some("85A12F01".to_string()));
        #[cfg(feature = "usbportinfo-interface")]
        assert_eq!(info.interface, Some(2));

        let ftdi_serial_hwid = r"FTDIBUS\VID_0403+PID_6001+A702TB52A\0000";
        let info = parse_usb_port_info(ftdi_serial_hwid, None).unwrap();

        assert_eq!(info.vid, 0x0403);
        assert_eq!(info.pid, 0x6001);
        assert_eq!(info.serial_number, Some("A702TB52A".to_string()));
        #[cfg(feature = "usbportinfo-interface")]
        assert_eq!(info.interface, None);

        let pyboard_hwid = r"USB\VID_F055&PID_9802\385435603432";
        let info = parse_usb_port_info(pyboard_hwid, None).unwrap();

        assert_eq!(info.vid, 0xF055);
        assert_eq!(info.pid, 0x9802);
        assert_eq!(info.serial_number, Some("385435603432".to_string()));
        #[cfg(feature = "usbportinfo-interface")]
        assert_eq!(info.interface, None);

        let unicode_serial = r"USB\VID_F055&PID_9802\3854356β03432&test";
        let info = parse_usb_port_info(unicode_serial, None).unwrap();
        assert_eq!(info.serial_number.as_deref(), Some("3854356β03432"));

        let unicode_serial = r"USB\VID_F055&PID_9802\3854356β03432";
        let info = parse_usb_port_info(unicode_serial, None).unwrap();
        assert_eq!(info.serial_number.as_deref(), Some("3854356β03432"));

        let unicode_serial = r"USB\VID_F055&PID_9802\3854356β";
        let info = parse_usb_port_info(unicode_serial, None).unwrap();
        assert_eq!(info.serial_number.as_deref(), Some("3854356β"));

        let serial_with_underscore_hwid = r"USB\VID_0483&PID_5740\TMCS_B000000000";
        let info = parse_usb_port_info(serial_with_underscore_hwid, None).unwrap();
        assert_eq!(info.serial_number.as_deref(), Some("TMCS_B000000000"));
    }
}
