//! Comprehensive OEM Hardware Heuristic Database
//! Matches Indian e-Governance biometrics (Mantra, Morpho, Startek), Iris scanners,
//! UVC Webcams, POS receipt printers, and COM serial devices with exact icons & flags.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DeviceCategory {
    FingerprintScanner,
    IrisScanner,
    Webcam,
    Printer,
    SerialPort,
    SmartCard,
    MassStorage,
    GenericUsb,
}

impl DeviceCategory {
    pub fn icon(&self) -> &'static str {
        match self {
            DeviceCategory::FingerprintScanner => "🪪",
            DeviceCategory::IrisScanner => "👁️",
            DeviceCategory::Webcam => "📷",
            DeviceCategory::Printer => "🖨️",
            DeviceCategory::SerialPort => "🔌",
            DeviceCategory::SmartCard => "🔐",
            DeviceCategory::MassStorage => "💾",
            DeviceCategory::GenericUsb => "⚡",
        }
    }

    pub fn display_name(&self) -> &'static str {
        match self {
            DeviceCategory::FingerprintScanner => "Biometric Fingerprint Scanner",
            DeviceCategory::IrisScanner => "Iris Biometric Scanner",
            DeviceCategory::Webcam => "HD Webcam / UVC Camera",
            DeviceCategory::Printer => "Printer & POS Terminal",
            DeviceCategory::SerialPort => "Serial / COM Port",
            DeviceCategory::SmartCard => "SmartCard & Security Token",
            DeviceCategory::MassStorage => "USB Mass Storage",
            DeviceCategory::GenericUsb => "USB Peripheral",
        }
    }
}

pub const FLAG_RD_SERVICE_OPTIMIZED: u32 = 0x0001; // Enable anticipatory URB pooling for Aadhaar RD Service
pub const FLAG_ISOCH_JITTER_BUFFER: u32  = 0x0002; // Enable microsecond-paced video frame reassembly
pub const FLAG_BULK_OUT_BURST: u32       = 0x0004; // Fast thermal print bitmap burst streaming
pub const FLAG_LOW_LATENCY_SERIAL: u32   = 0x0008; // Sub-1ms COM port baud synchronization

#[derive(Debug, Clone)]
pub struct OemDeviceRule {
    pub vid: u16,
    pub pid: Option<u16>, // None = Match all PIDs for this vendor
    pub vendor: &'static str,
    pub model: &'static str,
    pub category: DeviceCategory,
    pub flags: u32,
}

pub static OEM_DEVICE_RULES: &[OemDeviceRule] = &[
    // --- Mantra Softech Biometrics & Iris ---
    OemDeviceRule {
        vid: 0x27D0, pid: Some(0x0041),
        vendor: "Mantra Softech", model: "MFS100 Optical Fingerprint Scanner",
        category: DeviceCategory::FingerprintScanner, flags: FLAG_RD_SERVICE_OPTIMIZED,
    },
    OemDeviceRule {
        vid: 0x27D0, pid: Some(0x0040),
        vendor: "Mantra Softech", model: "MFS100 Fingerprint Scanner (Alternate PID)",
        category: DeviceCategory::FingerprintScanner, flags: FLAG_RD_SERVICE_OPTIMIZED,
    },
    OemDeviceRule {
        vid: 0x27D0, pid: Some(0x0050),
        vendor: "Mantra Softech", model: "MFS500 Fingerprint Scanner",
        category: DeviceCategory::FingerprintScanner, flags: FLAG_RD_SERVICE_OPTIMIZED,
    },
    OemDeviceRule {
        vid: 0x27D0, pid: Some(0x0042),
        vendor: "Mantra Softech", model: "MIS100V2 Dual Iris Scanner",
        category: DeviceCategory::IrisScanner, flags: FLAG_RD_SERVICE_OPTIMIZED | FLAG_ISOCH_JITTER_BUFFER,
    },
    OemDeviceRule {
        vid: 0x27D0, pid: Some(0x0045),
        vendor: "Mantra Softech", model: "MIS100V2 Dual Iris Scanner (L1)",
        category: DeviceCategory::IrisScanner, flags: FLAG_RD_SERVICE_OPTIMIZED | FLAG_ISOCH_JITTER_BUFFER,
    },

    // --- IDEMIA / Morpho Safran Biometrics ---
    OemDeviceRule {
        vid: 0x08B0, pid: Some(0x0031),
        vendor: "IDEMIA / Morpho", model: "Morpho MSO 1300 E2",
        category: DeviceCategory::FingerprintScanner, flags: FLAG_RD_SERVICE_OPTIMIZED,
    },
    OemDeviceRule {
        vid: 0x079B, pid: Some(0x0031),
        vendor: "IDEMIA / Morpho", model: "Morpho MSO 1300 E3 Smart Card & Biometric",
        category: DeviceCategory::FingerprintScanner, flags: FLAG_RD_SERVICE_OPTIMIZED,
    },
    OemDeviceRule {
        vid: 0x079B, pid: Some(0x0045),
        vendor: "IDEMIA / Morpho", model: "MorphoSmart MSO 1300 E2",
        category: DeviceCategory::FingerprintScanner, flags: FLAG_RD_SERVICE_OPTIMIZED,
    },
    OemDeviceRule {
        vid: 0x079B, pid: Some(0x0047),
        vendor: "IDEMIA / Morpho", model: "MorphoSmart MSO 1300 E3",
        category: DeviceCategory::FingerprintScanner, flags: FLAG_RD_SERVICE_OPTIMIZED,
    },

    // --- Startek & SecuGen Biometrics ---
    OemDeviceRule {
        vid: 0x1FBA, pid: Some(0x0013),
        vendor: "Startek", model: "FM220U Optical Fingerprint Scanner",
        category: DeviceCategory::FingerprintScanner, flags: FLAG_RD_SERVICE_OPTIMIZED,
    },
    OemDeviceRule {
        vid: 0x0BCA, pid: Some(0x8220),
        vendor: "Startek", model: "FM220 Fingerprint Scanner",
        category: DeviceCategory::FingerprintScanner, flags: FLAG_RD_SERVICE_OPTIMIZED,
    },
    OemDeviceRule {
        vid: 0x1162, pid: Some(0x0320),
        vendor: "SecuGen", model: "Hamster Pro 20 / HUPx",
        category: DeviceCategory::FingerprintScanner, flags: FLAG_RD_SERVICE_OPTIMIZED,
    },
    OemDeviceRule {
        vid: 0x1162, pid: Some(0x2200),
        vendor: "SecuGen", model: "Hamster IV Biometric Reader",
        category: DeviceCategory::FingerprintScanner, flags: FLAG_RD_SERVICE_OPTIMIZED,
    },

    // --- IriTech Iris Scanners ---
    OemDeviceRule {
        vid: 0x2655, pid: Some(0x0110),
        vendor: "IriTech", model: "IriShield MK2120U Dual Iris Camera",
        category: DeviceCategory::IrisScanner, flags: FLAG_RD_SERVICE_OPTIMIZED | FLAG_ISOCH_JITTER_BUFFER,
    },

    // --- Webcams / UVC Cameras ---
    OemDeviceRule {
        vid: 0x046D, pid: Some(0x082D),
        vendor: "Logitech", model: "HD Pro Webcam C920",
        category: DeviceCategory::Webcam, flags: FLAG_ISOCH_JITTER_BUFFER,
    },
    OemDeviceRule {
        vid: 0x046D, pid: Some(0x085E),
        vendor: "Logitech", model: "HD Pro Webcam C922",
        category: DeviceCategory::Webcam, flags: FLAG_ISOCH_JITTER_BUFFER,
    },
    OemDeviceRule {
        vid: 0x046D, pid: Some(0x0825),
        vendor: "Logitech", model: "Webcam C270",
        category: DeviceCategory::Webcam, flags: FLAG_ISOCH_JITTER_BUFFER,
    },

    // --- Thermal POS & Label Printers ---
    OemDeviceRule {
        vid: 0x04B8, pid: Some(0x0202),
        vendor: "Epson", model: "TM-T88 Thermal Receipt POS Printer",
        category: DeviceCategory::Printer, flags: FLAG_BULK_OUT_BURST,
    },
    OemDeviceRule {
        vid: 0x0FE6, pid: Some(0x811E),
        vendor: "TVS Electronics", model: "RP3200 Plus Thermal Receipt Printer",
        category: DeviceCategory::Printer, flags: FLAG_BULK_OUT_BURST,
    },
    OemDeviceRule {
        vid: 0x1208, pid: None, // All Zebra printers
        vendor: "Zebra", model: "Barcode / Label Thermal Printer",
        category: DeviceCategory::Printer, flags: FLAG_BULK_OUT_BURST,
    },
    OemDeviceRule {
        vid: 0x03F0, pid: None, // HP printers
        vendor: "HP", model: "LaserJet / DeskJet Printer",
        category: DeviceCategory::Printer, flags: FLAG_BULK_OUT_BURST,
    },

    // --- Serial / COM Port Bridges ---
    OemDeviceRule {
        vid: 0x0403, pid: Some(0x6001),
        vendor: "FTDI", model: "FT232R USB-to-UART Serial Converter",
        category: DeviceCategory::SerialPort, flags: FLAG_LOW_LATENCY_SERIAL,
    },
    OemDeviceRule {
        vid: 0x0403, pid: Some(0x6010),
        vendor: "FTDI", model: "FT2232 Dual Channel USB-Serial",
        category: DeviceCategory::SerialPort, flags: FLAG_LOW_LATENCY_SERIAL,
    },
    OemDeviceRule {
        vid: 0x10C4, pid: Some(0xEA60),
        vendor: "Silicon Labs", model: "CP2102 / CP2104 USB-to-UART Bridge",
        category: DeviceCategory::SerialPort, flags: FLAG_LOW_LATENCY_SERIAL,
    },
    OemDeviceRule {
        vid: 0x1A86, pid: Some(0x7523),
        vendor: "WCH", model: "CH340 USB-to-Serial Port",
        category: DeviceCategory::SerialPort, flags: FLAG_LOW_LATENCY_SERIAL,
    },
    OemDeviceRule {
        vid: 0x067B, pid: Some(0x2303),
        vendor: "Prolific", model: "PL2303 USB-to-Serial Controller",
        category: DeviceCategory::SerialPort, flags: FLAG_LOW_LATENCY_SERIAL,
    },

    // --- Security Tokens & SmartCards ---
    OemDeviceRule {
        vid: 0x1050, pid: None,
        vendor: "Yubico", model: "YubiKey FIDO / OTP Security Key",
        category: DeviceCategory::SmartCard, flags: 0,
    },
    OemDeviceRule {
        vid: 0x096E, pid: None,
        vendor: "Feitian", model: "ePass2003 / FIDO Security Token",
        category: DeviceCategory::SmartCard, flags: 0,
    },
];

pub struct HardwareClassifier;

impl HardwareClassifier {
    pub fn classify(vid: u16, pid: u16, reported_desc: &str) -> (DeviceCategory, String, u32) {
        // 1. Exact VID + PID match
        for rule in OEM_DEVICE_RULES {
            if rule.vid == vid {
                if let Some(target_pid) = rule.pid {
                    if target_pid == pid {
                        return (rule.category, format!("{} {}", rule.vendor, rule.model), rule.flags);
                    }
                }
            }
        }

        // 2. Vendor VID match
        for rule in OEM_DEVICE_RULES {
            if rule.vid == vid && rule.pid.is_none() {
                return (rule.category, format!("{} {}", rule.vendor, rule.model), rule.flags);
            }
        }

        // 3. String heuristics from USB Device Description
        let lower = reported_desc.to_lowercase();
        if lower.contains("fingerprint") || lower.contains("biometric") || lower.contains("mfs") || lower.contains("morpho") {
            return (DeviceCategory::FingerprintScanner, reported_desc.to_string(), FLAG_RD_SERVICE_OPTIMIZED);
        }
        if lower.contains("iris") {
            return (DeviceCategory::IrisScanner, reported_desc.to_string(), FLAG_RD_SERVICE_OPTIMIZED | FLAG_ISOCH_JITTER_BUFFER);
        }
        if lower.contains("camera") || lower.contains("webcam") || lower.contains("video") || lower.contains("uvc") {
            return (DeviceCategory::Webcam, reported_desc.to_string(), FLAG_ISOCH_JITTER_BUFFER);
        }
        if lower.contains("print") || lower.contains("pos") || lower.contains("receipt") || lower.contains("thermal") {
            return (DeviceCategory::Printer, reported_desc.to_string(), FLAG_BULK_OUT_BURST);
        }
        if lower.contains("serial") || lower.contains("uart") || lower.contains("ftdi") || lower.contains("ch340") {
            return (DeviceCategory::SerialPort, reported_desc.to_string(), FLAG_LOW_LATENCY_SERIAL);
        }
        if lower.contains("smartcard") || lower.contains("token") || lower.contains("yubikey") {
            return (DeviceCategory::SmartCard, reported_desc.to_string(), 0);
        }
        if lower.contains("storage") || lower.contains("flash") || lower.contains("drive") {
            return (DeviceCategory::MassStorage, reported_desc.to_string(), 0);
        }

        (DeviceCategory::GenericUsb, if reported_desc.is_empty() { format!("USB Device [VID:{:04X} PID:{:04X}]", vid, pid) } else { reported_desc.to_string() }, 0)
    }
}
