//! Comprehensive OEM Hardware Heuristic Database
//! Matches Indian e-Governance biometrics (Mantra, Morpho, Startek), Iris scanners,
//! UVC Webcams, POS receipt printers, and COM serial devices with exact icons & flags.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DeviceCategory {
    FingerprintScanner,
    SlapScanner,
    IrisScanner,
    GpsDevice,
    Webcam,
    Printer,
    SerialPort,
    SmartCard,
    MassStorage,
    GenericUsb,
    HumanInterface,
}

impl DeviceCategory {
    pub fn icon(&self) -> &'static str {
        match self {
            DeviceCategory::FingerprintScanner => "🪪",
            DeviceCategory::SlapScanner => "🖐️",
            DeviceCategory::IrisScanner => "👁️",
            DeviceCategory::GpsDevice => "🛰️",
            DeviceCategory::Webcam => "📷",
            DeviceCategory::Printer => "🖨️",
            DeviceCategory::SerialPort => "🔌",
            DeviceCategory::SmartCard => "🔐",
            DeviceCategory::MassStorage => "💾",
            DeviceCategory::GenericUsb => "⚡",
            DeviceCategory::HumanInterface => "⌨️",
        }
    }

    pub fn display_name(&self) -> &'static str {
        match self {
            DeviceCategory::FingerprintScanner => "Biometric Fingerprint Scanner",
            DeviceCategory::SlapScanner => "4:4:2 Ten-Print Slap Scanner",
            DeviceCategory::IrisScanner => "Iris Biometric Scanner",
            DeviceCategory::GpsDevice => "GPS / GNSS Geotagging Receiver",
            DeviceCategory::Webcam => "HD Webcam / UVC Camera",
            DeviceCategory::Printer => "Printer & POS Terminal",
            DeviceCategory::SerialPort => "Serial / COM Port",
            DeviceCategory::SmartCard => "SmartCard & Security Token",
            DeviceCategory::MassStorage => "USB Mass Storage",
            DeviceCategory::GenericUsb => "USB Peripheral",
            DeviceCategory::HumanInterface => "Keyboard / Mouse / HID (System)",
        }
    }
}

pub const FLAG_RD_SERVICE_OPTIMIZED: u32 = 0x0001; // Enable anticipatory URB pooling for Aadhaar RD Service
pub const FLAG_ISOCH_JITTER_BUFFER: u32  = 0x0002; // Enable microsecond-paced video frame reassembly
pub const FLAG_BULK_OUT_BURST: u32       = 0x0004; // Fast thermal print bitmap burst streaming
pub const FLAG_LOW_LATENCY_SERIAL: u32   = 0x0008; // Sub-1ms COM port baud synchronization
pub const FLAG_SYSTEM_DEVICE: u32        = 0x0010; // Primary system input device (keyboard/mouse) - excluded by default

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
    // --- 1. Dual Iris Scanners (e-KYC & Ayushman Bharat Certified) ---
    OemDeviceRule {
        vid: 0x27D0, pid: Some(0x0042),
        vendor: "Mantra Softech", model: "MIS100V2 Dual Iris Scanner",
        category: DeviceCategory::IrisScanner, flags: FLAG_RD_SERVICE_OPTIMIZED | FLAG_ISOCH_JITTER_BUFFER,
    },
    OemDeviceRule {
        vid: 0x27D0, pid: Some(0x0045),
        vendor: "Mantra Softech", model: "MATISX Dual Iris Scanner (L1)",
        category: DeviceCategory::IrisScanner, flags: FLAG_RD_SERVICE_OPTIMIZED | FLAG_ISOCH_JITTER_BUFFER,
    },
    OemDeviceRule {
        vid: 0x2B89, pid: Some(0x0020),
        vendor: "CMITECH", model: "BMT-20 Dual Iris Scanner",
        category: DeviceCategory::IrisScanner, flags: FLAG_RD_SERVICE_OPTIMIZED | FLAG_ISOCH_JITTER_BUFFER,
    },
    OemDeviceRule {
        vid: 0x2B89, pid: Some(0x0030),
        vendor: "CMITECH", model: "EMX-30 Dual Iris Scanner",
        category: DeviceCategory::IrisScanner, flags: FLAG_RD_SERVICE_OPTIMIZED | FLAG_ISOCH_JITTER_BUFFER,
    },
    OemDeviceRule {
        vid: 0x2655, pid: Some(0x0110),
        vendor: "IriTech", model: "IriShield MK2120U Dual Iris Camera",
        category: DeviceCategory::IrisScanner, flags: FLAG_RD_SERVICE_OPTIMIZED | FLAG_ISOCH_JITTER_BUFFER,
    },
    OemDeviceRule {
        vid: 0x2655, pid: Some(0x0120),
        vendor: "IriTech", model: "IriUniverse / BK2121U Dual Iris Scanner",
        category: DeviceCategory::IrisScanner, flags: FLAG_RD_SERVICE_OPTIMIZED | FLAG_ISOCH_JITTER_BUFFER,
    },

    // --- 2. Fingerprint Slap Scanners (4:4:2 Ten-Print for Aadhaar & Ayushman Bharat) ---
    OemDeviceRule {
        vid: 0x27D0, pid: Some(0x0060),
        vendor: "Mantra Softech", model: "MFS600 4:4:2 Ten-Print Slap Scanner",
        category: DeviceCategory::SlapScanner, flags: FLAG_RD_SERVICE_OPTIMIZED,
    },
    OemDeviceRule {
        vid: 0x27D0, pid: Some(0x0061),
        vendor: "Mantra Softech", model: "MFS600 Slap Scanner (Optical)",
        category: DeviceCategory::SlapScanner, flags: FLAG_RD_SERVICE_OPTIMIZED,
    },
    OemDeviceRule {
        vid: 0x079B, pid: Some(0x0033),
        vendor: "IDEMIA / Morpho", model: "MorphoTop 100 Ten-Print Slap Scanner",
        category: DeviceCategory::SlapScanner, flags: FLAG_RD_SERVICE_OPTIMIZED,
    },
    OemDeviceRule {
        vid: 0x079B, pid: Some(0x0034),
        vendor: "IDEMIA / Morpho", model: "MorphoTop MORPHS Ten-Print Slap Scanner",
        category: DeviceCategory::SlapScanner, flags: FLAG_RD_SERVICE_OPTIMIZED,
    },
    OemDeviceRule {
        vid: 0x08B0, pid: Some(0x0033),
        vendor: "IDEMIA / Morpho", model: "MorphoTop 100 Live Scanner",
        category: DeviceCategory::SlapScanner, flags: FLAG_RD_SERVICE_OPTIMIZED,
    },
    OemDeviceRule {
        vid: 0x05BA, pid: Some(0x000A),
        vendor: "Thales / 3M Cogent", model: "Cogent CS500e Ten-Print Slap Scanner",
        category: DeviceCategory::SlapScanner, flags: FLAG_RD_SERVICE_OPTIMIZED,
    },
    OemDeviceRule {
        vid: 0x05BA, pid: Some(0x000B),
        vendor: "Thales / 3M Cogent", model: "Cogent CSD550 Slap Scanner",
        category: DeviceCategory::SlapScanner, flags: FLAG_RD_SERVICE_OPTIMIZED,
    },
    OemDeviceRule {
        vid: 0x05BA, pid: Some(0x0010),
        vendor: "Thales / 3M Cogent", model: "DactyScan84c 4:4:2 Slap Scanner",
        category: DeviceCategory::SlapScanner, flags: FLAG_RD_SERVICE_OPTIMIZED,
    },

    // --- 3. GPS / GNSS Geotagging Receivers (UIDAI Aadhaar ECMP Station Geofencing) ---
    OemDeviceRule {
        vid: 0x067B, pid: Some(0x2303),
        vendor: "GlobalSat", model: "BU-353-S4 USB GPS Receiver",
        category: DeviceCategory::GpsDevice, flags: FLAG_LOW_LATENCY_SERIAL,
    },
    OemDeviceRule {
        vid: 0x067B, pid: Some(0x23A3),
        vendor: "GlobalSat", model: "G-Star IV GPS Receiver",
        category: DeviceCategory::GpsDevice, flags: FLAG_LOW_LATENCY_SERIAL,
    },
    OemDeviceRule {
        vid: 0x1546, pid: Some(0x01A7),
        vendor: "u-blox", model: "7 / 8 / M8 GNSS Receiver",
        category: DeviceCategory::GpsDevice, flags: FLAG_LOW_LATENCY_SERIAL,
    },
    OemDeviceRule {
        vid: 0x1546, pid: Some(0x01A8),
        vendor: "BioEnable / u-blox", model: "G2 GNSS Geotagging Receiver",
        category: DeviceCategory::GpsDevice, flags: FLAG_LOW_LATENCY_SERIAL,
    },
    OemDeviceRule {
        vid: 0x1546, pid: None,
        vendor: "u-blox", model: "GNSS / GPS Receiver",
        category: DeviceCategory::GpsDevice, flags: FLAG_LOW_LATENCY_SERIAL,
    },

    // --- 4. Single Biometric Fingerprint Scanners (L1 & L0 Certified) ---
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
        vid: 0x27D0, pid: Some(0x0110),
        vendor: "Mantra Softech", model: "MFS110 L1 Biometric Fingerprint Scanner",
        category: DeviceCategory::FingerprintScanner, flags: FLAG_RD_SERVICE_OPTIMIZED,
    },
    OemDeviceRule {
        vid: 0x27D0, pid: Some(0x0043),
        vendor: "Mantra Softech", model: "MFS110 L1 Optical Fingerprint Scanner (Alternate PID)",
        category: DeviceCategory::FingerprintScanner, flags: FLAG_RD_SERVICE_OPTIMIZED,
    },
    OemDeviceRule {
        vid: 0x27D0, pid: Some(0x0050),
        vendor: "Mantra Softech", model: "MFS500 Fingerprint Scanner",
        category: DeviceCategory::FingerprintScanner, flags: FLAG_RD_SERVICE_OPTIMIZED,
    },
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
    OemDeviceRule {
        vid: 0x079B, pid: Some(0x0048),
        vendor: "IDEMIA / Morpho", model: "MorphoSmart MSO 1300 E3 L1",
        category: DeviceCategory::FingerprintScanner, flags: FLAG_RD_SERVICE_OPTIMIZED,
    },
    OemDeviceRule {
        vid: 0x1FBA, pid: Some(0x0013),
        vendor: "Startek", model: "FM220U Optical Fingerprint Scanner",
        category: DeviceCategory::FingerprintScanner, flags: FLAG_RD_SERVICE_OPTIMIZED,
    },
    OemDeviceRule {
        vid: 0x1FBA, pid: Some(0x0014),
        vendor: "Startek", model: "FM220U L1 Biometric Fingerprint Scanner",
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
        vid: 0x1162, pid: Some(0x0330),
        vendor: "SecuGen", model: "Hamster Pro 10 / HU10 Optical Reader",
        category: DeviceCategory::FingerprintScanner, flags: FLAG_RD_SERVICE_OPTIMIZED,
    },
    OemDeviceRule {
        vid: 0x1162, pid: Some(0x2200),
        vendor: "SecuGen", model: "Hamster IV Biometric Reader",
        category: DeviceCategory::FingerprintScanner, flags: FLAG_RD_SERVICE_OPTIMIZED,
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
        let lower = reported_desc.to_lowercase();

        // 1. High-priority string heuristics for specialized functions overriding generic UART bridges
        // (e.g. CP2102, FTDI, CH340 dongles configured as NMEA GPS / GNSS receivers)
        if lower.contains("gps") || lower.contains("gnss") || lower.contains("nmea") || lower.contains("geotag") || lower.contains("sirf") {
            return (
                DeviceCategory::GpsDevice,
                if reported_desc.is_empty() { "Generic NMEA GPS Receiver".to_string() } else { reported_desc.to_string() },
                FLAG_LOW_LATENCY_SERIAL,
            );
        }

        // 2. Exact VID + PID match
        for rule in OEM_DEVICE_RULES {
            if rule.vid == vid {
                if let Some(target_pid) = rule.pid {
                    if target_pid == pid {
                        return (rule.category, format!("{} {}", rule.vendor, rule.model), rule.flags);
                    }
                }
            }
        }

        // 3. Vendor VID match
        for rule in OEM_DEVICE_RULES {
            if rule.vid == vid && rule.pid.is_none() {
                return (rule.category, format!("{} {}", rule.vendor, rule.model), rule.flags);
            }
        }

        // 4. String heuristics from USB Device Description
        if lower.contains("slap") || lower.contains("ten-print") || lower.contains("tenprint")
            || lower.contains("morphotop") || lower.contains("cs500") || lower.contains("csd550") || lower.contains("dactyscan") {
            return (DeviceCategory::SlapScanner, reported_desc.to_string(), FLAG_RD_SERVICE_OPTIMIZED);
        }
        if lower.contains("iris") || lower.contains("matisx") || lower.contains("mis100")
            || lower.contains("bmt-20") || lower.contains("emx-30") || lower.contains("irishield") || lower.contains("iriuniverse") {
            return (DeviceCategory::IrisScanner, reported_desc.to_string(), FLAG_RD_SERVICE_OPTIMIZED | FLAG_ISOCH_JITTER_BUFFER);
        }
        if lower.contains("fingerprint") || lower.contains("biometric") || lower.contains("mfs") || lower.contains("morpho")
            || lower.contains("fm220") || lower.contains("secugen") || lower.contains("hamster") {
            return (DeviceCategory::FingerprintScanner, reported_desc.to_string(), FLAG_RD_SERVICE_OPTIMIZED);
        }
        if lower.contains("camera") || lower.contains("webcam") || lower.contains("video") || lower.contains("uvc") {
            return (DeviceCategory::Webcam, reported_desc.to_string(), FLAG_ISOCH_JITTER_BUFFER);
        }
        if lower.contains("print") || lower.contains("pos") || lower.contains("receipt") || lower.contains("thermal") {
            return (DeviceCategory::Printer, reported_desc.to_string(), FLAG_BULK_OUT_BURST);
        }
        if lower.contains("serial") || lower.contains("uart") || lower.contains("ftdi") || lower.contains("ch340") || lower.contains("pl2303") || lower.contains("cp210") {
            return (DeviceCategory::SerialPort, reported_desc.to_string(), FLAG_LOW_LATENCY_SERIAL);
        }
        if lower.contains("smartcard") || lower.contains("token") || lower.contains("yubikey") {
            return (DeviceCategory::SmartCard, reported_desc.to_string(), 0);
        }
        if lower.contains("storage") || lower.contains("flash") || lower.contains("drive") {
            return (DeviceCategory::MassStorage, reported_desc.to_string(), 0);
        }
        if lower.contains("keyboard") || lower.contains("mouse") || lower.contains("touchpad") || lower.contains("trackball") || lower.contains("pointing") {
            return (DeviceCategory::HumanInterface, reported_desc.to_string(), FLAG_SYSTEM_DEVICE);
        }

        (DeviceCategory::GenericUsb, if reported_desc.is_empty() { format!("USB Device [VID:{:04X} PID:{:04X}]", vid, pid) } else { reported_desc.to_string() }, 0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_device_category_icons_and_display_names() {
        assert_eq!(DeviceCategory::GpsDevice.icon(), "🛰️");
        assert_eq!(DeviceCategory::GpsDevice.display_name(), "GPS / GNSS Geotagging Receiver");
        assert_eq!(DeviceCategory::SlapScanner.icon(), "🖐️");
        assert_eq!(DeviceCategory::SlapScanner.display_name(), "4:4:2 Ten-Print Slap Scanner");
        assert_eq!(DeviceCategory::FingerprintScanner.icon(), "🪪");
        assert_eq!(DeviceCategory::IrisScanner.icon(), "👁️");
    }

    #[test]
    fn test_dual_iris_scanners_classification() {
        // Mantra MATISX & MIS100V2 (VID: 0x27D0, PID: 0x0042, 0x0045)
        let (cat, desc, flags) = HardwareClassifier::classify(0x27D0, 0x0042, "");
        assert_eq!(cat, DeviceCategory::IrisScanner);
        assert_eq!(flags, FLAG_RD_SERVICE_OPTIMIZED | FLAG_ISOCH_JITTER_BUFFER);
        assert!(desc.contains("MIS100V2"));

        let (cat, desc, flags) = HardwareClassifier::classify(0x27D0, 0x0045, "");
        assert_eq!(cat, DeviceCategory::IrisScanner);
        assert_eq!(flags, FLAG_RD_SERVICE_OPTIMIZED | FLAG_ISOCH_JITTER_BUFFER);
        assert!(desc.contains("MATISX"));

        // CMITECH BMT-20 & EMX-30 (VID: 0x2B89, PID: 0x0020, 0x0030)
        let (cat, desc, flags) = HardwareClassifier::classify(0x2B89, 0x0020, "");
        assert_eq!(cat, DeviceCategory::IrisScanner);
        assert_eq!(flags, FLAG_RD_SERVICE_OPTIMIZED | FLAG_ISOCH_JITTER_BUFFER);
        assert!(desc.contains("BMT-20"));

        let (cat, desc, flags) = HardwareClassifier::classify(0x2B89, 0x0030, "");
        assert_eq!(cat, DeviceCategory::IrisScanner);
        assert_eq!(flags, FLAG_RD_SERVICE_OPTIMIZED | FLAG_ISOCH_JITTER_BUFFER);
        assert!(desc.contains("EMX-30"));

        // IriTech IriUniverse & IriShield MK2120U / BK2121U (VID: 0x2655, PID: 0x0110, 0x0120)
        let (cat, desc, flags) = HardwareClassifier::classify(0x2655, 0x0110, "");
        assert_eq!(cat, DeviceCategory::IrisScanner);
        assert_eq!(flags, FLAG_RD_SERVICE_OPTIMIZED | FLAG_ISOCH_JITTER_BUFFER);
        assert!(desc.contains("MK2120U"));

        let (cat, desc, flags) = HardwareClassifier::classify(0x2655, 0x0120, "");
        assert_eq!(cat, DeviceCategory::IrisScanner);
        assert_eq!(flags, FLAG_RD_SERVICE_OPTIMIZED | FLAG_ISOCH_JITTER_BUFFER);
        assert!(desc.contains("BK2121U") || desc.contains("IriUniverse"));
    }

    #[test]
    fn test_fingerprint_slap_scanners_classification() {
        // Mantra MFS600 (VID: 0x27D0, PID: 0x0060, 0x0061)
        let (cat, desc, flags) = HardwareClassifier::classify(0x27D0, 0x0060, "");
        assert_eq!(cat, DeviceCategory::SlapScanner);
        assert_eq!(flags, FLAG_RD_SERVICE_OPTIMIZED);
        assert!(desc.contains("MFS600"));

        let (cat, desc, flags) = HardwareClassifier::classify(0x27D0, 0x0061, "");
        assert_eq!(cat, DeviceCategory::SlapScanner);
        assert_eq!(flags, FLAG_RD_SERVICE_OPTIMIZED);
        assert!(desc.contains("MFS600"));

        // Idemia / Safran Morpho MorphoTop 100 / MORPHS (VID: 0x079B, PID: 0x0033, 0x0034; VID: 0x08B0, PID: 0x0033)
        let (cat, desc, flags) = HardwareClassifier::classify(0x079B, 0x0033, "");
        assert_eq!(cat, DeviceCategory::SlapScanner);
        assert_eq!(flags, FLAG_RD_SERVICE_OPTIMIZED);
        assert!(desc.contains("MorphoTop 100"));

        let (cat, desc, flags) = HardwareClassifier::classify(0x079B, 0x0034, "");
        assert_eq!(cat, DeviceCategory::SlapScanner);
        assert_eq!(flags, FLAG_RD_SERVICE_OPTIMIZED);
        assert!(desc.contains("MorphoTop MORPHS"));

        let (cat, desc, flags) = HardwareClassifier::classify(0x08B0, 0x0033, "");
        assert_eq!(cat, DeviceCategory::SlapScanner);
        assert_eq!(flags, FLAG_RD_SERVICE_OPTIMIZED);
        assert!(desc.contains("MorphoTop 100"));

        // Thales / 3M Cogent CS500e / CSD550 / DactyScan84c (VID: 0x05BA, PID: 0x000A, 0x000B, 0x0010)
        let (cat, desc, flags) = HardwareClassifier::classify(0x05BA, 0x000A, "");
        assert_eq!(cat, DeviceCategory::SlapScanner);
        assert_eq!(flags, FLAG_RD_SERVICE_OPTIMIZED);
        assert!(desc.contains("CS500e"));

        let (cat, desc, flags) = HardwareClassifier::classify(0x05BA, 0x000B, "");
        assert_eq!(cat, DeviceCategory::SlapScanner);
        assert_eq!(flags, FLAG_RD_SERVICE_OPTIMIZED);
        assert!(desc.contains("CSD550"));

        let (cat, desc, flags) = HardwareClassifier::classify(0x05BA, 0x0010, "");
        assert_eq!(cat, DeviceCategory::SlapScanner);
        assert_eq!(flags, FLAG_RD_SERVICE_OPTIMIZED);
        assert!(desc.contains("DactyScan84c"));
    }

    #[test]
    fn test_gps_gnss_devices_classification() {
        // GlobalSat BU-353-S4 & G-Star IV (Prolific PL2303 / SiRFstar IV - VID: 0x067B, PID: 0x2303, 0x23A3)
        let (cat, desc, flags) = HardwareClassifier::classify(0x067B, 0x2303, "");
        assert_eq!(cat, DeviceCategory::GpsDevice);
        assert_eq!(flags, FLAG_LOW_LATENCY_SERIAL);
        assert!(desc.contains("BU-353-S4"));

        let (cat, desc, flags) = HardwareClassifier::classify(0x067B, 0x23A3, "");
        assert_eq!(cat, DeviceCategory::GpsDevice);
        assert_eq!(flags, FLAG_LOW_LATENCY_SERIAL);
        assert!(desc.contains("G-Star IV"));

        // BioEnable G2 GNSS / u-blox USB GPS (u-blox 7/8/M8 - VID: 0x1546, PID: 0x01A7, 0x01A8)
        let (cat, desc, flags) = HardwareClassifier::classify(0x1546, 0x01A7, "");
        assert_eq!(cat, DeviceCategory::GpsDevice);
        assert_eq!(flags, FLAG_LOW_LATENCY_SERIAL);
        assert!(desc.contains("u-blox"));

        let (cat, desc, flags) = HardwareClassifier::classify(0x1546, 0x01A8, "");
        assert_eq!(cat, DeviceCategory::GpsDevice);
        assert_eq!(flags, FLAG_LOW_LATENCY_SERIAL);
        assert!(desc.contains("BioEnable") || desc.contains("G2 GNSS"));

        // Generic NMEA GPS Serial (CP2102, FTDI, CH340 GPS dongles)
        let (cat, desc, flags) = HardwareClassifier::classify(0x10C4, 0xEA60, "Generic NMEA GPS Serial (CP2102)");
        assert_eq!(cat, DeviceCategory::GpsDevice);
        assert_eq!(flags, FLAG_LOW_LATENCY_SERIAL);
        assert!(desc.contains("GPS"));

        let (cat, desc, flags) = HardwareClassifier::classify(0x0403, 0x6001, "FTDI NMEA GPS Engine");
        assert_eq!(cat, DeviceCategory::GpsDevice);
        assert_eq!(flags, FLAG_LOW_LATENCY_SERIAL);
        assert!(desc.contains("GPS"));

        let (cat, desc, flags) = HardwareClassifier::classify(0x1A86, 0x7523, "CH340 GNSS Receiver Dongle");
        assert_eq!(cat, DeviceCategory::GpsDevice);
        assert_eq!(flags, FLAG_LOW_LATENCY_SERIAL);
        assert!(desc.contains("GNSS"));
    }

    #[test]
    fn test_single_biometric_scanners_classification() {
        // Mantra MFS110 (L1 RD Service - VID: 0x27D0, PID: 0x0110, 0x0043)
        let (cat, desc, flags) = HardwareClassifier::classify(0x27D0, 0x0110, "");
        assert_eq!(cat, DeviceCategory::FingerprintScanner);
        assert_eq!(flags, FLAG_RD_SERVICE_OPTIMIZED);
        assert!(desc.contains("MFS110"));

        let (cat, desc, flags) = HardwareClassifier::classify(0x27D0, 0x0043, "");
        assert_eq!(cat, DeviceCategory::FingerprintScanner);
        assert_eq!(flags, FLAG_RD_SERVICE_OPTIMIZED);
        assert!(desc.contains("MFS110"));

        // Idemia / Morpho MSO 1300 E3 (L1 - VID: 0x079B, PID: 0x0047, 0x0048)
        let (cat, desc, flags) = HardwareClassifier::classify(0x079B, 0x0047, "");
        assert_eq!(cat, DeviceCategory::FingerprintScanner);
        assert_eq!(flags, FLAG_RD_SERVICE_OPTIMIZED);
        assert!(desc.contains("MSO 1300 E3"));

        let (cat, desc, flags) = HardwareClassifier::classify(0x079B, 0x0048, "");
        assert_eq!(cat, DeviceCategory::FingerprintScanner);
        assert_eq!(flags, FLAG_RD_SERVICE_OPTIMIZED);
        assert!(desc.contains("MSO 1300 E3 L1"));

        // Startek FM220U (L1 & L0 - VID: 0x1FBA, PID: 0x0013, 0x0014; VID: 0x0BCA, PID: 0x8220)
        let (cat, desc, flags) = HardwareClassifier::classify(0x1FBA, 0x0013, "");
        assert_eq!(cat, DeviceCategory::FingerprintScanner);
        assert_eq!(flags, FLAG_RD_SERVICE_OPTIMIZED);
        assert!(desc.contains("FM220U"));

        let (cat, desc, flags) = HardwareClassifier::classify(0x1FBA, 0x0014, "");
        assert_eq!(cat, DeviceCategory::FingerprintScanner);
        assert_eq!(flags, FLAG_RD_SERVICE_OPTIMIZED);
        assert!(desc.contains("FM220U L1"));

        let (cat, desc, flags) = HardwareClassifier::classify(0x0BCA, 0x8220, "");
        assert_eq!(cat, DeviceCategory::FingerprintScanner);
        assert_eq!(flags, FLAG_RD_SERVICE_OPTIMIZED);
        assert!(desc.contains("FM220"));

        // SecuGen Hamster Pro 20 / 10 (VID: 0x1162, PID: 0x0320, 0x0330)
        let (cat, desc, flags) = HardwareClassifier::classify(0x1162, 0x0320, "");
        assert_eq!(cat, DeviceCategory::FingerprintScanner);
        assert_eq!(flags, FLAG_RD_SERVICE_OPTIMIZED);
        assert!(desc.contains("Hamster Pro 20"));

        let (cat, desc, flags) = HardwareClassifier::classify(0x1162, 0x0330, "");
        assert_eq!(cat, DeviceCategory::FingerprintScanner);
        assert_eq!(flags, FLAG_RD_SERVICE_OPTIMIZED);
        assert!(desc.contains("Hamster Pro 10"));
    }
}
