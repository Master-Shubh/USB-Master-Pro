//! WinUSB Client & Physical URB Dispatcher
//! Bridges incoming remote Guest URBs directly to local physical USB hardware via WinUSB.
//! Enforces strict single-device ownership, pipe timeouts, cancellation, and stall resets.

use super::device::{WinUsbDevice, WinUsbError};
use super::ffi::*;
use crate::protocol::{TransferType, TurboUrbHeader, CMD_UNLINK_URB};
use crate::urb_engine::{
    USBD_STATUS_CANCELED, USBD_STATUS_DEV_NOT_RESPONDING, USBD_STATUS_STALL_PID,
    USBD_STATUS_SUCCESS,
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};

pub const DEFAULT_WINUSB_TEST_GUID: &str = "{dee824ef-729b-4a0e-9c14-b7117d33a817}";
pub const STATUS_NOT_SUPPORTED: i32 = 0xC00000BB_u32 as i32;
pub const STATUS_DEVICE_NOT_CONNECTED: i32 = 0xC000000E_u32 as i32;
pub const STATUS_TIMEOUT: i32 = 0x00000102_u32 as i32;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WinUsbDeviceInfo {
    pub path: String,
    pub vid: u16,
    pub pid: u16,
    pub serial_number: String,
    pub interface_guid: String,
    pub friendly_name: String,
}

pub struct WinUsbClient {
    devices: Mutex<HashMap<u32, Arc<WinUsbDevice>>>,
    default_device_id: AtomicU32,
    #[cfg(any(test, feature = "driver-simulation"))]
    simulation_responses: Mutex<HashMap<u64, (i32, Vec<u8>)>>,
}

impl Default for WinUsbClient {
    fn default() -> Self {
        Self::new()
    }
}

impl WinUsbClient {
    pub fn new() -> Self {
        Self {
            devices: Mutex::new(HashMap::new()),
            default_device_id: AtomicU32::new(0),
            #[cfg(any(test, feature = "driver-simulation"))]
            simulation_responses: Mutex::new(HashMap::new()),
        }
    }

    /// Enumerates all present devices exposing the specified WinUSB device interface GUID.
    pub fn enumerate_devices(guid_str: Option<&str>) -> Vec<WinUsbDeviceInfo> {
        let target_guid_str = guid_str.unwrap_or(DEFAULT_WINUSB_TEST_GUID);
        let guid = match GUID::from_str_guid(target_guid_str) {
            Ok(g) => g,
            Err(_) => return Vec::new(),
        };

        #[cfg(not(target_os = "windows"))]
        {
            let _ = guid;
            Vec::new()
        }

        #[cfg(target_os = "windows")]
        unsafe {
            let mut list = Vec::new();
            let dev_info = SetupDiGetClassDevsW(
                &guid,
                std::ptr::null(),
                std::ptr::null_mut(),
                DIGCF_PRESENT | DIGCF_DEVICEINTERFACE,
            );

            if dev_info == INVALID_HANDLE_VALUE || dev_info.is_null() {
                return list;
            }

            let mut iface_data = SP_DEVICE_INTERFACE_DATA {
                cb_size: std::mem::size_of::<SP_DEVICE_INTERFACE_DATA>() as u32,
                interface_class_guid: GUID::default(),
                flags: 0,
                reserved: 0,
            };

            let mut member_index = 0u32;
            while SetupDiEnumDeviceInterfaces(
                dev_info,
                std::ptr::null_mut(),
                &guid,
                member_index,
                &mut iface_data,
            ) != 0
            {
                member_index += 1;

                // First call: retrieve required buffer size
                let mut req_size = 0u32;
                SetupDiGetDeviceInterfaceDetailW(
                    dev_info,
                    &mut iface_data,
                    std::ptr::null_mut(),
                    0,
                    &mut req_size,
                    std::ptr::null_mut(),
                );

                if req_size == 0 {
                    continue;
                }

                // Allocate buffer for SP_DEVICE_INTERFACE_DETAIL_DATA_W
                // cb_size is 8 on 64-bit Windows, 5/6 on 32-bit Windows
                let mut detail_buffer = vec![0u8; req_size as usize];
                let cb_size: u32 = if std::mem::size_of::<usize>() == 8 { 8 } else { 6 };
                detail_buffer[0..4].copy_from_slice(&cb_size.to_ne_bytes());

                let mut dev_data = SP_DEVINFO_DATA {
                    cb_size: std::mem::size_of::<SP_DEVINFO_DATA>() as u32,
                    class_guid: GUID::default(),
                    dev_inst: 0,
                    reserved: 0,
                };

                if SetupDiGetDeviceInterfaceDetailW(
                    dev_info,
                    &mut iface_data,
                    detail_buffer.as_mut_ptr(),
                    req_size,
                    std::ptr::null_mut(),
                    &mut dev_data,
                ) != 0
                {
                    // DevicePath starts at offset 4
                    let path_u16_slice = std::slice::from_raw_parts(
                        detail_buffer[4..].as_ptr() as *const u16,
                        ((req_size as usize).saturating_sub(4)) / 2,
                    );
                    let mut path = String::from_utf16_lossy(path_u16_slice);
                    if let Some(null_idx) = path.find('\0') {
                        path.truncate(null_idx);
                    }

                    // Extract VID, PID, Serial
                    let upper_path = path.to_uppercase();
                    let mut vid = 0u16;
                    let mut pid = 0u16;
                    if let Some(pos) = upper_path.find("VID_") {
                        if upper_path.len() >= pos + 8 {
                            vid = u16::from_str_radix(&upper_path[pos + 4..pos + 8], 16).unwrap_or(0);
                        }
                    }
                    if let Some(pos) = upper_path.find("PID_") {
                        if upper_path.len() >= pos + 8 {
                            pid = u16::from_str_radix(&upper_path[pos + 4..pos + 8], 16).unwrap_or(0);
                        }
                    }

                    list.push(WinUsbDeviceInfo {
                        path: path.clone(),
                        vid,
                        pid,
                        serial_number: "Physical WinUSB Device".to_string(),
                        interface_guid: target_guid_str.to_string(),
                        friendly_name: format!("WinUSB Device [{:04X}:{:04X}]", vid, pid),
                    });
                }
            }

            SetupDiDestroyDeviceInfoList(dev_info);
            list
        }
    }

    /// Claims and opens a physical WinUSB device, associating it with a session device ID.
    pub fn claim_device(&self, device_id: u32, path: &str) -> Result<Arc<WinUsbDevice>, WinUsbError> {
        let dev = WinUsbDevice::open(path)?;
        let arc_dev = Arc::new(dev);
        let mut map = self.devices.lock().unwrap();
        map.insert(device_id, Arc::clone(&arc_dev));
        self.default_device_id.store(device_id, Ordering::SeqCst);
        Ok(arc_dev)
    }

    /// Releases a claimed WinUSB device.
    pub fn release_device(&self, device_id: u32) {
        let mut map = self.devices.lock().unwrap();
        map.remove(&device_id);
    }

    /// Gets an open device by device ID (or returns the default device if only one is active).
    pub fn get_device(&self, device_id: u32) -> Option<Arc<WinUsbDevice>> {
        let map = self.devices.lock().unwrap();
        if let Some(d) = map.get(&device_id) {
            Some(Arc::clone(d))
        } else if map.len() == 1 {
            map.values().next().cloned()
        } else {
            None
        }
    }

    #[cfg(any(test, feature = "driver-simulation"))]
    pub fn register_simulation_response(&self, sequence_id: u64, status: i32, data: Vec<u8>) {
        let mut map = self.simulation_responses.lock().unwrap();
        map.insert(sequence_id, (status, data));
    }

    /// Dispatches an incoming URB submission to the physical USB hardware.
    /// Returns `(status: i32, response_data: Vec<u8>)`.
    pub fn dispatch_urb(
        &self,
        header: &TurboUrbHeader,
        setup_packet: Option<&[u8; 8]>,
        payload: &[u8],
    ) -> (i32, Vec<u8>) {
        #[cfg(any(test, feature = "driver-simulation"))]
        {
            let mut sim = self.simulation_responses.lock().unwrap();
            if let Some(resp) = sim.remove(&header.sequence_id) {
                return resp;
            }
        }

        // Check for UNLINK / Abort request
        if header.command == CMD_UNLINK_URB {
            if let Some(dev) = self.get_device(header.device_id) {
                let _ = dev.abort_pipe(header.endpoint_address);
            }
            return (USBD_STATUS_CANCELED as i32, Vec::new());
        }

        let dev = match self.get_device(header.device_id) {
            Some(d) => d,
            None => {
                // If device is not opened, check if any WinUSB device can be auto-claimed
                let discovered = Self::enumerate_devices(None);
                if let Some(first) = discovered.first() {
                    match self.claim_device(header.device_id, &first.path) {
                        Ok(d) => d,
                        Err(e) => {
                            eprintln!("⚠️ Failed to auto-claim WinUSB device: {}", e);
                            return (STATUS_DEVICE_NOT_CONNECTED, Vec::new());
                        }
                    }
                } else {
                    return (STATUS_DEVICE_NOT_CONNECTED, Vec::new());
                }
            }
        };

        match header.transfer_type {
            TransferType::Control => {
                let sp = match setup_packet {
                    Some(s) => s,
                    None => {
                        eprintln!("⚠️ Control transfer missing 8-byte setup packet for Seq {}", header.sequence_id);
                        return (STATUS_NOT_SUPPORTED, Vec::new());
                    }
                };

                let is_in = (sp[0] & 0x80) != 0;
                let expected_len = (header.actual_length as usize).min(65536);

                if is_in {
                    let mut buffer = vec![0u8; expected_len.max(u16::from_le_bytes([sp[6], sp[7]]) as usize)];
                    match dev.control_transfer(sp, &mut buffer, 3000) {
                        Ok(transferred) => {
                            buffer.truncate(transferred as usize);
                            (USBD_STATUS_SUCCESS as i32, buffer)
                        }
                        Err(WinUsbError::Timeout { .. }) => (STATUS_TIMEOUT, Vec::new()),
                        Err(WinUsbError::Stalled(ep)) => {
                            let _ = dev.reset_pipe(ep);
                            (USBD_STATUS_STALL_PID as i32, Vec::new())
                        }
                        Err(WinUsbError::DeviceDisconnected) => {
                            self.release_device(header.device_id);
                            (STATUS_DEVICE_NOT_CONNECTED, Vec::new())
                        }
                        Err(e) => {
                            eprintln!("⚠️ Control IN transfer failed: {}", e);
                            (USBD_STATUS_DEV_NOT_RESPONDING as i32, Vec::new())
                        }
                    }
                } else {
                    // Control OUT: copy payload into mutable buffer
                    let mut buffer = payload.to_vec();
                    match dev.control_transfer(sp, &mut buffer, 3000) {
                        Ok(_) => (USBD_STATUS_SUCCESS as i32, Vec::new()),
                        Err(WinUsbError::Timeout { .. }) => (STATUS_TIMEOUT, Vec::new()),
                        Err(WinUsbError::Stalled(ep)) => {
                            let _ = dev.reset_pipe(ep);
                            (USBD_STATUS_STALL_PID as i32, Vec::new())
                        }
                        Err(WinUsbError::DeviceDisconnected) => {
                            self.release_device(header.device_id);
                            (STATUS_DEVICE_NOT_CONNECTED, Vec::new())
                        }
                        Err(e) => {
                            eprintln!("⚠️ Control OUT transfer failed: {}", e);
                            (USBD_STATUS_DEV_NOT_RESPONDING as i32, Vec::new())
                        }
                    }
                }
            }
            TransferType::Bulk | TransferType::Interrupt => {
                let is_in = (header.endpoint_address & 0x80) != 0;
                if is_in {
                    let req_len = (header.actual_length as usize).min(65536);
                    let mut buffer = vec![0u8; req_len];
                    match dev.read_pipe(header.endpoint_address, &mut buffer, 3000) {
                        Ok(transferred) => {
                            buffer.truncate(transferred as usize);
                            (USBD_STATUS_SUCCESS as i32, buffer)
                        }
                        Err(WinUsbError::Timeout { .. }) => (STATUS_TIMEOUT, Vec::new()),
                        Err(WinUsbError::Stalled(ep)) => {
                            let _ = dev.reset_pipe(ep);
                            (USBD_STATUS_STALL_PID as i32, Vec::new())
                        }
                        Err(WinUsbError::DeviceDisconnected) => {
                            self.release_device(header.device_id);
                            (STATUS_DEVICE_NOT_CONNECTED, Vec::new())
                        }
                        Err(e) => {
                            eprintln!("⚠️ Pipe read failed on EP 0x{:02X}: {}", header.endpoint_address, e);
                            (USBD_STATUS_DEV_NOT_RESPONDING as i32, Vec::new())
                        }
                    }
                } else {
                    // Bulk / Interrupt OUT
                    match dev.write_pipe(header.endpoint_address, payload, 3000) {
                        Ok(_) => (USBD_STATUS_SUCCESS as i32, Vec::new()),
                        Err(WinUsbError::Timeout { .. }) => (STATUS_TIMEOUT, Vec::new()),
                        Err(WinUsbError::Stalled(ep)) => {
                            let _ = dev.reset_pipe(ep);
                            (USBD_STATUS_STALL_PID as i32, Vec::new())
                        }
                        Err(WinUsbError::DeviceDisconnected) => {
                            self.release_device(header.device_id);
                            (STATUS_DEVICE_NOT_CONNECTED, Vec::new())
                        }
                        Err(e) => {
                            eprintln!("⚠️ Pipe write failed on EP 0x{:02X}: {}", header.endpoint_address, e);
                            (USBD_STATUS_DEV_NOT_RESPONDING as i32, Vec::new())
                        }
                    }
                }
            }
            TransferType::Isochronous => {
                // WinUSB standard user-mode does not support Isochronous transfers (requires WinUSB RAW_IO Isoch on Win 8.1+)
                eprintln!("⚠️ Isochronous transfer not supported on standard WinUSB profile");
                (STATUS_NOT_SUPPORTED, Vec::new())
            }
        }
    }
}
