//! PnP Hardware Device Discovery & State Enumerator
//! Discovers all connected USB devices, hubs, printers, biometrics, and serial ports.
//! Features physical hardware sysfs/SetupAPI querying and primary system device exclusion.

use crate::hardware_db::{DeviceCategory, HardwareClassifier, FLAG_SYSTEM_DEVICE};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UsbDeviceInfo {
    pub id: String,                  // Unique hardware identifier e.g. "1-4.2"
    pub vid: u16,                    // Vendor ID
    pub pid: u16,                    // Product ID
    pub bus_num: u8,
    pub dev_num: u8,
    pub port_path: String,           // Physical hub topology e.g. "Port 1-4"
    pub friendly_name: String,       // Human-readable classified name
    pub vendor_name: String,
    pub product_name: String,
    pub serial_number: String,
    pub category: DeviceCategory,
    pub category_icon: String,
    pub speed: String,               // "Low-Speed", "Full-Speed", "High-Speed", "SuperSpeed"
    pub is_shared: bool,             // Currently offered for remote connection
    pub is_mounted: bool,            // Forwarded to and active on remote peer
    pub remote_peer_id: Option<String>,
    pub flags: u32,
    #[serde(default)]
    pub is_system_device: bool,      // Flagged if primary system keyboard/mouse/HID
    #[serde(default = "default_can_forward")]
    pub can_forward: bool,           // Non-forwardable by default if system device unless forced
}

fn default_can_forward() -> bool {
    true
}

pub struct HardwareEnumerator;

impl HardwareEnumerator {
    /// Scans the machine's USB subsystem and returns a structured list of classified devices.
    /// Strictly returns real physical devices. Never injects fake or sample devices.
    pub fn scan_devices() -> Vec<UsbDeviceInfo> {
        let mut devices = Vec::new();

        // 1. Scan Linux /sys/bus/usb/devices (Runs on Linux hosts and testing environments)
        if Path::new("/sys/bus/usb/devices").exists() {
            devices.extend(Self::scan_sysfs());
        }

        // 2. If running on Windows, native SetupAPI is called
        #[cfg(target_os = "windows")]
        {
            devices.extend(Self::scan_windows_setupapi());
        }

        devices
    }

    /// Scans Linux sysfs for connected USB peripherals
    fn scan_sysfs() -> Vec<UsbDeviceInfo> {
        let mut list = Vec::new();
        let path = Path::new("/sys/bus/usb/devices");

        if let Ok(entries) = fs::read_dir(path) {
            for entry in entries.flatten() {
                let dev_path = entry.path();
                let dirname = dev_path.file_name().unwrap_or_default().to_string_lossy().to_string();

                // Skip root hubs like "usb1", "usb2" and interface subdirs like "1-1:1.0"
                if dirname.starts_with("usb") || dirname.contains(':') {
                    continue;
                }

                // Check device class: skip USB Hubs (Class 09)
                let b_dev_class = fs::read_to_string(dev_path.join("bDeviceClass")).unwrap_or_default().trim().to_string();
                if b_dev_class == "09" {
                    continue;
                }

                let id_vendor = fs::read_to_string(dev_path.join("idVendor")).unwrap_or_default().trim().to_string();
                let id_product = fs::read_to_string(dev_path.join("idProduct")).unwrap_or_default().trim().to_string();

                if id_vendor.is_empty() || id_product.is_empty() {
                    continue;
                }

                let vid = u16::from_str_radix(&id_vendor, 16).unwrap_or(0);
                let pid = u16::from_str_radix(&id_product, 16).unwrap_or(0);

                let manufacturer = fs::read_to_string(dev_path.join("manufacturer")).unwrap_or_default().trim().to_string();
                let product = fs::read_to_string(dev_path.join("product")).unwrap_or_default().trim().to_string();
                let serial = fs::read_to_string(dev_path.join("serial")).unwrap_or_default().trim().to_string();
                let speed_raw = fs::read_to_string(dev_path.join("speed")).unwrap_or_default().trim().to_string();

                let bus_num = fs::read_to_string(dev_path.join("busnum")).unwrap_or_default().trim().parse::<u8>().unwrap_or(1);
                let dev_num = fs::read_to_string(dev_path.join("devnum")).unwrap_or_default().trim().parse::<u8>().unwrap_or(1);

                let speed = match speed_raw.as_str() {
                    "1.5" => "USB 1.1 (Low-Speed 1.5 Mbps)",
                    "12" => "USB 1.1 (Full-Speed 12 Mbps)",
                    "480" => "USB 2.0 (High-Speed 480 Mbps)",
                    "5000" => "USB 3.0 (SuperSpeed 5 Gbps)",
                    "10000" => "USB 3.1 (SuperSpeed+ 10 Gbps)",
                    _ => "USB 2.0 (High-Speed)",
                }.to_string();

                let desc = if !product.is_empty() {
                    format!("{} {}", manufacturer, product).trim().to_string()
                } else {
                    format!("USB Device [{:04X}:{:04X}]", vid, pid)
                };

                let (mut category, friendly_name, mut flags) = HardwareClassifier::classify(vid, pid, &desc);

                // --- DEVICE EXCLUSION & SYSTEM DEVICE DETECTION ---
                // Detect primary system keyboard, mouse, or touchpad to prevent locking user out
                let mut is_system_device = (flags & FLAG_SYSTEM_DEVICE) != 0;
                let mut is_mouse_or_keyboard = false;

                // Inspect interface descriptors under device (e.g., 2-1:1.0/bInterfaceClass)
                if let Ok(subentries) = fs::read_dir(&dev_path) {
                    for subentry in subentries.flatten() {
                        let sub_name = subentry.file_name().to_string_lossy().to_string();
                        if sub_name.starts_with(&format!("{}:", dirname)) {
                            let if_class = fs::read_to_string(subentry.path().join("bInterfaceClass"))
                                .unwrap_or_default().trim().to_string();
                            let if_proto = fs::read_to_string(subentry.path().join("bInterfaceProtocol"))
                                .unwrap_or_default().trim().to_string();

                            // HID class = 03, protocol 01 = keyboard, 02 = mouse
                            if if_class == "03" {
                                is_system_device = true;
                                if if_proto == "01" || if_proto == "02" {
                                    is_mouse_or_keyboard = true;
                                }
                            }
                        }
                    }
                }

                let desc_lower = desc.to_lowercase();
                if desc_lower.contains("keyboard")
                    || desc_lower.contains("mouse")
                    || desc_lower.contains("touchpad")
                    || desc_lower.contains("trackpoint")
                    || desc_lower.contains("trackball")
                    || desc_lower.contains("pointing")
                {
                    is_system_device = true;
                    is_mouse_or_keyboard = true;
                }

                if is_system_device {
                    flags |= FLAG_SYSTEM_DEVICE;
                    if category == DeviceCategory::GenericUsb {
                        category = DeviceCategory::HumanInterface;
                    }
                }

                let can_forward = !is_system_device;
                let category_icon = if is_mouse_or_keyboard {
                    if desc_lower.contains("mouse") { "🖱️".to_string() } else { "⌨️".to_string() }
                } else {
                    category.icon().to_string()
                };

                list.push(UsbDeviceInfo {
                    id: dirname.clone(),
                    vid,
                    pid,
                    bus_num,
                    dev_num,
                    port_path: format!("Port {}", dirname),
                    friendly_name,
                    vendor_name: if manufacturer.is_empty() { "OEM Vendor".to_string() } else { manufacturer },
                    product_name: if product.is_empty() { format!("Device {:04X}:{:04X}", vid, pid) } else { product },
                    serial_number: if serial.is_empty() { "N/A".to_string() } else { serial },
                    category,
                    category_icon,
                    speed,
                    is_shared: false,
                    is_mounted: false,
                    remote_peer_id: None,
                    flags,
                    is_system_device,
                    can_forward,
                });
            }
        }

        list
    }

    /// Windows SetupAPI native device enumeration
    #[cfg(target_os = "windows")]
    pub fn scan_windows_setupapi() -> Vec<UsbDeviceInfo> {
        let mut list = Vec::new();

        unsafe {
            let enumerator: Vec<u16> = "USB\0".encode_utf16().collect();
            let dev_info = win_setupapi::SetupDiGetClassDevsW(
                std::ptr::null(),
                enumerator.as_ptr(),
                std::ptr::null_mut(),
                win_setupapi::DIGCF_PRESENT | win_setupapi::DIGCF_ALLCLASSES,
            );

            if dev_info == win_setupapi::INVALID_HANDLE_VALUE {
                return list;
            }

            let mut dev_data = win_setupapi::SP_DEVINFO_DATA {
                cb_size: std::mem::size_of::<win_setupapi::SP_DEVINFO_DATA>() as u32,
                class_guid: [0u8; 16],
                dev_inst: 0,
                reserved: 0,
            };

            let mut idx = 0u32;
            while win_setupapi::SetupDiEnumDeviceInfo(dev_info, idx, &mut dev_data) != 0 {
                idx += 1;

                let hw_id = win_setupapi::get_device_property(dev_info, &mut dev_data, win_setupapi::SPDRP_HARDWAREID);
                let friendly_name_raw = win_setupapi::get_device_property(dev_info, &mut dev_data, win_setupapi::SPDRP_FRIENDLYNAME);
                let device_desc = win_setupapi::get_device_property(dev_info, &mut dev_data, win_setupapi::SPDRP_DEVICEDESC);
                let mfg = win_setupapi::get_device_property(dev_info, &mut dev_data, win_setupapi::SPDRP_MFG);
                let dev_class = win_setupapi::get_device_property(dev_info, &mut dev_data, win_setupapi::SPDRP_CLASS);

                // Parse VID and PID from HardwareID (e.g. "USB\VID_27D0&PID_0041...")
                let mut vid = 0u16;
                let mut pid = 0u16;
                let upper_hw = hw_id.to_uppercase();
                if let Some(vid_pos) = upper_hw.find("VID_") {
                    if upper_hw.len() >= vid_pos + 8 {
                        vid = u16::from_str_radix(&upper_hw[vid_pos + 4..vid_pos + 8], 16).unwrap_or(0);
                    }
                }
                if let Some(pid_pos) = upper_hw.find("PID_") {
                    if upper_hw.len() >= pid_pos + 8 {
                        pid = u16::from_str_radix(&upper_hw[pid_pos + 4..pid_pos + 8], 16).unwrap_or(0);
                    }
                }

                if vid == 0 && pid == 0 {
                    continue;
                }

                let desc = if !friendly_name_raw.is_empty() {
                    friendly_name_raw.clone()
                } else if !device_desc.is_empty() {
                    device_desc.clone()
                } else {
                    format!("USB Device [{:04X}:{:04X}]", vid, pid)
                };

                let (mut category, classified_name, mut flags) = HardwareClassifier::classify(vid, pid, &desc);

                let friendly_name = if !friendly_name_raw.is_empty() {
                    friendly_name_raw
                } else {
                    classified_name
                };

                let class_lower = dev_class.to_lowercase();
                let desc_lower = desc.to_lowercase();
                let is_mouse = desc_lower.contains("mouse") || class_lower.contains("mouse");
                let is_keyboard = desc_lower.contains("keyboard") || class_lower.contains("keyboard");
                let is_system_device = is_mouse || is_keyboard || class_lower == "hidclass" || (flags & FLAG_SYSTEM_DEVICE) != 0;

                if is_system_device {
                    flags |= FLAG_SYSTEM_DEVICE;
                    if category == DeviceCategory::GenericUsb {
                        category = DeviceCategory::HumanInterface;
                    }
                }

                let category_icon = if is_mouse {
                    "🖱️".to_string()
                } else if is_keyboard {
                    "⌨️".to_string()
                } else {
                    category.icon().to_string()
                };

                list.push(UsbDeviceInfo {
                    id: format!("win-usb-{}", idx),
                    vid,
                    pid,
                    bus_num: 1,
                    dev_num: idx as u8,
                    port_path: format!("Bus 1 Port {}", idx),
                    friendly_name,
                    vendor_name: if mfg.is_empty() { "OEM Vendor".to_string() } else { mfg },
                    product_name: desc,
                    serial_number: "N/A".to_string(),
                    category,
                    category_icon,
                    speed: "USB 2.0 (High-Speed)".to_string(),
                    is_shared: false,
                    is_mounted: false,
                    remote_peer_id: None,
                    flags,
                    is_system_device,
                    can_forward: !is_system_device,
                });
            }

            win_setupapi::SetupDiDestroyDeviceInfoList(dev_info);
        }

        list
    }

    /// Non-Windows stub ensuring scan_windows_setupapi is always present without E0599
    #[cfg(not(target_os = "windows"))]
    pub fn scan_windows_setupapi() -> Vec<UsbDeviceInfo> {
        Vec::new()
    }
}

#[cfg(target_os = "windows")]
mod win_setupapi {
    use std::ffi::c_void;

    pub type HDEVINFO = *mut c_void;
    pub const INVALID_HANDLE_VALUE: HDEVINFO = -1isize as HDEVINFO;
    pub const DIGCF_PRESENT: u32 = 0x00000002;
    pub const DIGCF_ALLCLASSES: u32 = 0x00000004;

    pub const SPDRP_DEVICEDESC: u32 = 0x00000000;
    pub const SPDRP_HARDWAREID: u32 = 0x00000001;
    pub const SPDRP_CLASS: u32 = 0x00000007;
    pub const SPDRP_MFG: u32 = 0x0000000B;
    pub const SPDRP_FRIENDLYNAME: u32 = 0x0000000C;

    #[repr(C)]
    pub struct SP_DEVINFO_DATA {
        pub cb_size: u32,
        pub class_guid: [u8; 16],
        pub dev_inst: u32,
        pub reserved: usize,
    }

    #[link(name = "setupapi")]
    extern "system" {
        pub fn SetupDiGetClassDevsW(
            class_guid: *const c_void,
            enumerator: *const u16,
            hwnd_parent: *mut c_void,
            flags: u32,
        ) -> HDEVINFO;

        pub fn SetupDiEnumDeviceInfo(
            device_info_set: HDEVINFO,
            member_index: u32,
            device_info_data: *mut SP_DEVINFO_DATA,
        ) -> i32;

        pub fn SetupDiGetDeviceRegistryPropertyW(
            device_info_set: HDEVINFO,
            device_info_data: *const SP_DEVINFO_DATA,
            property: u32,
            property_reg_data_type: *mut u32,
            property_buffer: *mut u8,
            property_buffer_size: u32,
            required_size: *mut u32,
        ) -> i32;

        pub fn SetupDiDestroyDeviceInfoList(device_info_set: HDEVINFO) -> i32;
    }

    pub unsafe fn get_device_property(
        dev_info: HDEVINFO,
        dev_data: *mut SP_DEVINFO_DATA,
        property: u32,
    ) -> String {
        let mut buffer = [0u8; 1024];
        let mut req_size = 0u32;
        let res = SetupDiGetDeviceRegistryPropertyW(
            dev_info,
            dev_data,
            property,
            std::ptr::null_mut(),
            buffer.as_mut_ptr(),
            buffer.len() as u32,
            &mut req_size,
        );
        if res != 0 && req_size >= 2 {
            let u16_slice = std::slice::from_raw_parts(
                buffer.as_ptr() as *const u16,
                (req_size as usize / 2).saturating_sub(1),
            );
            String::from_utf16_lossy(u16_slice).trim().to_string()
        } else {
            String::new()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_scan_devices_never_returns_fake_sample_devices() {
        let devices = HardwareEnumerator::scan_devices();
        // Ensure no hardcoded mock devices (e.g. 749219 or 1932E00418) are ever injected
        for dev in &devices {
            assert_ne!(dev.serial_number, "MFS100_749219", "Fake mock Mantra device detected!");
            assert_ne!(dev.serial_number, "1932E00418", "Fake mock Morpho device detected!");
        }
    }

    #[test]
    fn test_system_exclusion_logic() {
        let devices = HardwareEnumerator::scan_devices();
        for dev in &devices {
            if dev.friendly_name.to_lowercase().contains("mouse")
                || dev.product_name.to_lowercase().contains("mouse")
                || dev.friendly_name.to_lowercase().contains("keyboard")
                || dev.product_name.to_lowercase().contains("keyboard")
            {
                assert!(dev.is_system_device, "System input device {} must be flagged as system device", dev.friendly_name);
                assert!(!dev.can_forward, "System input device {} must not be forwardable by default", dev.friendly_name);
            }
        }
    }
}

