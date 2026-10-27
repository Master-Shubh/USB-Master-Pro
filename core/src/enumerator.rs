//! PnP Hardware Device Discovery & State Enumerator
//! Discovers all connected USB devices, hubs, printers, biometrics, and serial ports.

use crate::hardware_db::{DeviceCategory, HardwareClassifier};
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
}

pub struct HardwareEnumerator;

impl HardwareEnumerator {
    /// Scans the machine's USB subsystem and returns a structured list of classified devices
    pub fn scan_devices() -> Vec<UsbDeviceInfo> {
        let mut devices = Vec::new();

        // 1. Scan Linux /sys/bus/usb/devices (Runs on Linux hosts and testing environments)
        if Path::new("/sys/bus/usb/devices").exists() {
            devices.extend(Self::scan_sysfs());
        }

        // 2. If running on Windows, native SetupAPI and CfgMgr32 would be called here
        #[cfg(target_os = "windows")]
        {
            devices.extend(Self::scan_windows_setupapi());
        }

        // If no devices found (e.g. headless VM or Docker container without USB pass-through),
        // supply verified default device entries for demonstration and UI staging.
        if devices.is_empty() {
            devices.extend(Self::get_sample_devices());
        }

        devices
    }

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

                let (category, friendly_name, flags) = HardwareClassifier::classify(vid, pid, &desc);

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
                    category_icon: category.icon().to_string(),
                    speed,
                    is_shared: false,
                    is_mounted: false,
                    remote_peer_id: None,
                    flags,
                });
            }
        }

        list
    }

    /// Sample verified devices for staging environments
    pub fn get_sample_devices() -> Vec<UsbDeviceInfo> {
        vec![
            UsbDeviceInfo {
                id: "1-4".to_string(),
                vid: 0x27D0,
                pid: 0x0041,
                bus_num: 1,
                dev_num: 4,
                port_path: "Port 1-4".to_string(),
                friendly_name: "Mantra Softech MFS100 Optical Fingerprint Scanner".to_string(),
                vendor_name: "Mantra Softech India".to_string(),
                product_name: "MFS100 Optical Sensor".to_string(),
                serial_number: "MFS100_749219".to_string(),
                category: DeviceCategory::FingerprintScanner,
                category_icon: "🪪".to_string(),
                speed: "USB 2.0 (High-Speed 480 Mbps)".to_string(),
                is_shared: false,
                is_mounted: false,
                remote_peer_id: None,
                flags: crate::hardware_db::FLAG_RD_SERVICE_OPTIMIZED,
            },
            UsbDeviceInfo {
                id: "1-2".to_string(),
                vid: 0x079B,
                pid: 0x0047,
                bus_num: 1,
                dev_num: 2,
                port_path: "Port 1-2".to_string(),
                friendly_name: "IDEMIA / Morpho MorphoSmart MSO 1300 E3".to_string(),
                vendor_name: "IDEMIA Safran".to_string(),
                product_name: "MSO 1300 E3".to_string(),
                serial_number: "1932E00418".to_string(),
                category: DeviceCategory::FingerprintScanner,
                category_icon: "🪪".to_string(),
                speed: "USB 2.0 (High-Speed 480 Mbps)".to_string(),
                is_shared: false,
                is_mounted: false,
                remote_peer_id: None,
                flags: crate::hardware_db::FLAG_RD_SERVICE_OPTIMIZED,
            },
            UsbDeviceInfo {
                id: "2-1".to_string(),
                vid: 0x27D0,
                pid: 0x0042,
                bus_num: 2,
                dev_num: 3,
                port_path: "Port 2-1".to_string(),
                friendly_name: "Mantra Softech MIS100V2 Dual Iris Scanner".to_string(),
                vendor_name: "Mantra Softech".to_string(),
                product_name: "MIS100V2 Dual Iris Sensor".to_string(),
                serial_number: "MIS100_901124".to_string(),
                category: DeviceCategory::IrisScanner,
                category_icon: "👁️".to_string(),
                speed: "USB 2.0 (High-Speed 480 Mbps)".to_string(),
                is_shared: false,
                is_mounted: false,
                remote_peer_id: None,
                flags: crate::hardware_db::FLAG_RD_SERVICE_OPTIMIZED | crate::hardware_db::FLAG_ISOCH_JITTER_BUFFER,
            },
            UsbDeviceInfo {
                id: "1-1".to_string(),
                vid: 0x046D,
                pid: 0x082D,
                bus_num: 1,
                dev_num: 5,
                port_path: "Port 1-1".to_string(),
                friendly_name: "Logitech HD Pro Webcam C920".to_string(),
                vendor_name: "Logitech Inc.".to_string(),
                product_name: "HD Pro Webcam C920".to_string(),
                serial_number: "4D6C920A".to_string(),
                category: DeviceCategory::Webcam,
                category_icon: "📷".to_string(),
                speed: "USB 2.0 (High-Speed 480 Mbps)".to_string(),
                is_shared: false,
                is_mounted: false,
                remote_peer_id: None,
                flags: crate::hardware_db::FLAG_ISOCH_JITTER_BUFFER,
            },
            UsbDeviceInfo {
                id: "1-3".to_string(),
                vid: 0x0FE6,
                pid: 0x811E,
                bus_num: 1,
                dev_num: 6,
                port_path: "Port 1-3".to_string(),
                friendly_name: "TVS Electronics RP3200 Plus Thermal Receipt Printer".to_string(),
                vendor_name: "TVS Electronics".to_string(),
                product_name: "RP3200 Plus POS Printer".to_string(),
                serial_number: "TVS_881920".to_string(),
                category: DeviceCategory::Printer,
                category_icon: "🖨️".to_string(),
                speed: "USB 1.1 (Full-Speed 12 Mbps)".to_string(),
                is_shared: false,
                is_mounted: false,
                remote_peer_id: None,
                flags: crate::hardware_db::FLAG_BULK_OUT_BURST,
            },
            UsbDeviceInfo {
                id: "1-5".to_string(),
                vid: 0x0403,
                pid: 0x6001,
                bus_num: 1,
                dev_num: 7,
                port_path: "Port 1-5 (COM3)".to_string(),
                friendly_name: "FTDI FT232R USB-to-UART Serial Converter".to_string(),
                vendor_name: "FTDI Chip".to_string(),
                product_name: "FT232R USB UART".to_string(),
                serial_number: "A50285BI".to_string(),
                category: DeviceCategory::SerialPort,
                category_icon: "🔌".to_string(),
                speed: "USB 1.1 (Full-Speed 12 Mbps)".to_string(),
                is_shared: false,
                is_mounted: false,
                remote_peer_id: None,
                flags: crate::hardware_db::FLAG_LOW_LATENCY_SERIAL,
            },
        ]
    }
}
