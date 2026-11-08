//! WinUSB User-Mode Physical Device Backend
//! Provides direct hardware communication with Tier 1 WinUSB-bound peripherals (e.g., RP2040 Pico test board).
//! Owned & Developed by BhartX Ecosystems Pvt Ltd.

pub mod client;
pub mod device;
pub mod ffi;

pub use client::{WinUsbClient, WinUsbDeviceInfo, DEFAULT_WINUSB_TEST_GUID};
pub use device::{UsbPipeInfo, WinUsbDevice, WinUsbError};
pub use ffi::GUID;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::{TransferType, TurboUrbHeader, CMD_SUBMIT_URB, CMD_UNLINK_URB};

    #[test]
    fn test_guid_parsing() {
        let guid_str = "{dee824ef-729b-4a0e-9c14-b7117d33a817}";
        let guid = GUID::from_str_guid(guid_str).expect("Valid GUID string must parse");
        assert_eq!(guid.data1, 0xdee824ef);
        assert_eq!(guid.data2, 0x729b);
        assert_eq!(guid.data3, 0x4a0e);
        assert_eq!(guid.data4, [0x9c, 0x14, 0xb7, 0x11, 0x7d, 0x33, 0xa8, 0x17]);

        let invalid_guid = "{invalid-guid-string}";
        assert!(GUID::from_str_guid(invalid_guid).is_err());
    }

    #[test]
    fn test_client_unlink_dispatch() {
        let client = WinUsbClient::new();
        let hdr = TurboUrbHeader::new(CMD_UNLINK_URB, 99, 1, 0x81, TransferType::Bulk, 0);
        let (status, data) = client.dispatch_urb(&hdr, None, &[]);
        assert_eq!(status, crate::urb_engine::USBD_STATUS_CANCELED as i32);
        assert!(data.is_empty());
    }

    #[test]
    fn test_client_simulation_response() {
        let client = WinUsbClient::new();
        let hdr = TurboUrbHeader::new(CMD_SUBMIT_URB, 101, 1, 0x80, TransferType::Control, 18);
        let mock_device_desc = vec![18, 1, 0x00, 0x02, 0, 0, 0, 64, 0xFE, 0xCA, 0x01, 0x40, 0, 1, 1, 2, 3, 1];
        
        #[cfg(any(test, feature = "driver-simulation"))]
        client.register_simulation_response(101, 0, mock_device_desc.clone());

        #[cfg(any(test, feature = "driver-simulation"))]
        {
            let (status, data) = client.dispatch_urb(&hdr, Some(&[0x80, 0x06, 0x00, 0x01, 0x00, 0x00, 18, 0]), &[]);
            assert_eq!(status, 0);
            assert_eq!(data, mock_device_desc);
        }
    }
}

