//! Win32 & WinUSB FFI Declarations
//! Native binding to winusb.dll, kernel32.dll, and setupapi.dll.
//! Includes mock stubs for non-Windows platforms to ensure cross-platform test builds.

use std::ffi::c_void;

pub type HANDLE = *mut c_void;
pub type WINUSB_INTERFACE_HANDLE = *mut c_void;
pub type BOOL = i32;

pub const INVALID_HANDLE_VALUE: HANDLE = -1isize as HANDLE;
pub const WAIT_OBJECT_0: u32 = 0x00000000;
pub const WAIT_TIMEOUT: u32 = 0x00000102;
pub const WAIT_FAILED: u32 = 0xFFFFFFFF;

pub const GENERIC_READ: u32 = 0x80000000;
pub const GENERIC_WRITE: u32 = 0x40000000;
pub const FILE_SHARE_READ: u32 = 0x00000001;
pub const FILE_SHARE_WRITE: u32 = 0x00000002;
pub const OPEN_EXISTING: u32 = 3;
pub const FILE_ATTRIBUTE_NORMAL: u32 = 0x00000080;
pub const FILE_FLAG_OVERLAPPED: u32 = 0x40000000;

// WinUSB Pipe Policies
pub const SHORT_PACKET_TERMINATE: u32 = 0x01;
pub const AUTO_CLEAR_STALL: u32 = 0x02;
pub const PIPE_TRANSFER_TIMEOUT: u32 = 0x03;
pub const IGNORE_SHORT_PACKETS: u32 = 0x04;
pub const SHORT_PACKET_OK: u32 = 0x05;
pub const RAW_IO: u32 = 0x07;

// SetupAPI Flags
pub const DIGCF_DEFAULT: u32 = 0x00000001;
pub const DIGCF_PRESENT: u32 = 0x00000002;
pub const DIGCF_ALLCLASSES: u32 = 0x00000004;
pub const DIGCF_PROFILE: u32 = 0x00000008;
pub const DIGCF_DEVICEINTERFACE: u32 = 0x00000010;

#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct GUID {
    pub data1: u32,
    pub data2: u16,
    pub data3: u16,
    pub data4: [u8; 8],
}

impl GUID {
    pub const fn from_raw(data1: u32, data2: u16, data3: u16, data4: [u8; 8]) -> Self {
        Self { data1, data2, data3, data4 }
    }

    /// Parses a standard Windows GUID string format "{xxxxxxxx-xxxx-xxxx-xxxx-xxxxxxxxxxxx}"
    pub fn from_str_guid(s: &str) -> Result<Self, String> {
        let clean = s.trim().trim_matches(|c| c == '{' || c == '}');
        let parts: Vec<&str> = clean.split('-').collect();
        if parts.len() != 5 {
            return Err(format!("Invalid GUID string: {}", s));
        }

        let data1 = u32::from_str_radix(parts[0], 16)
            .map_err(|e| format!("Invalid GUID data1: {}", e))?;
        let data2 = u16::from_str_radix(parts[1], 16)
            .map_err(|e| format!("Invalid GUID data2: {}", e))?;
        let data3 = u16::from_str_radix(parts[2], 16)
            .map_err(|e| format!("Invalid GUID data3: {}", e))?;

        if parts[3].len() != 4 || parts[4].len() != 12 {
            return Err(format!("Invalid GUID tail: {}", s));
        }

        let b1 = u8::from_str_radix(&parts[3][0..2], 16).map_err(|e| e.to_string())?;
        let b2 = u8::from_str_radix(&parts[3][2..4], 16).map_err(|e| e.to_string())?;
        let b3 = u8::from_str_radix(&parts[4][0..2], 16).map_err(|e| e.to_string())?;
        let b4 = u8::from_str_radix(&parts[4][2..4], 16).map_err(|e| e.to_string())?;
        let b5 = u8::from_str_radix(&parts[4][4..6], 16).map_err(|e| e.to_string())?;
        let b6 = u8::from_str_radix(&parts[4][6..8], 16).map_err(|e| e.to_string())?;
        let b7 = u8::from_str_radix(&parts[4][8..10], 16).map_err(|e| e.to_string())?;
        let b8 = u8::from_str_radix(&parts[4][10..12], 16).map_err(|e| e.to_string())?;

        Ok(Self {
            data1,
            data2,
            data3,
            data4: [b1, b2, b3, b4, b5, b6, b7, b8],
        })
    }
}

/// Standard WinUSB 8-byte setup packet (naturally 8 bytes, 2-byte aligned)
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct WINUSB_SETUP_PACKET {
    pub request_type: u8,
    pub request: u8,
    pub value: u16,
    pub index: u16,
    pub length: u16,
}

#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct WINUSB_PIPE_INFORMATION {
    pub pipe_type: u32,
    pub pipe_id: u8,
    pub maximum_packet_size: u16,
    pub interval: u8,
}

#[repr(C)]
pub struct SP_DEVINFO_DATA {
    pub cb_size: u32,
    pub class_guid: GUID,
    pub dev_inst: u32,
    pub reserved: usize,
}

#[repr(C)]
pub struct SP_DEVICE_INTERFACE_DATA {
    pub cb_size: u32,
    pub interface_class_guid: GUID,
    pub flags: u32,
    pub reserved: usize,
}

#[repr(C)]
pub struct OVERLAPPED {
    pub internal: usize,
    pub internal_high: usize,
    pub offset_low: u32,
    pub offset_high: u32,
    pub h_event: HANDLE,
}

impl Default for OVERLAPPED {
    fn default() -> Self {
        Self {
            internal: 0,
            internal_high: 0,
            offset_low: 0,
            offset_high: 0,
            h_event: std::ptr::null_mut(),
        }
    }
}

#[cfg(target_os = "windows")]
#[link(name = "kernel32")]
extern "system" {
    pub fn CreateFileW(
        file_name: *const u16,
        desired_access: u32,
        share_mode: u32,
        security_attributes: *mut c_void,
        creation_disposition: u32,
        flags_and_attributes: u32,
        template_file: HANDLE,
    ) -> HANDLE;
    pub fn CloseHandle(handle: HANDLE) -> BOOL;
    pub fn CreateEventW(
        security_attributes: *mut c_void,
        manual_reset: BOOL,
        initial_state: BOOL,
        name: *const u16,
    ) -> HANDLE;
    pub fn ResetEvent(event: HANDLE) -> BOOL;
    pub fn WaitForSingleObject(handle: HANDLE, milliseconds: u32) -> u32;
    pub fn GetOverlappedResult(
        handle: HANDLE,
        overlapped: *mut OVERLAPPED,
        number_of_bytes_transferred: *mut u32,
        wait: BOOL,
    ) -> BOOL;
    pub fn CancelIoEx(handle: HANDLE, overlapped: *mut OVERLAPPED) -> BOOL;
    pub fn GetLastError() -> u32;
}

#[cfg(target_os = "windows")]
#[link(name = "winusb")]
extern "system" {
    pub fn WinUsb_Initialize(
        device_handle: HANDLE,
        interface_handle: *mut WINUSB_INTERFACE_HANDLE,
    ) -> BOOL;
    pub fn WinUsb_Free(interface_handle: WINUSB_INTERFACE_HANDLE) -> BOOL;
    pub fn WinUsb_GetDescriptor(
        interface_handle: WINUSB_INTERFACE_HANDLE,
        descriptor_type: u8,
        index: u8,
        language_id: u16,
        buffer: *mut u8,
        buffer_length: u32,
        length_transferred: *mut u32,
    ) -> BOOL;
    pub fn WinUsb_QueryPipe(
        interface_handle: WINUSB_INTERFACE_HANDLE,
        alternate_interface_number: u8,
        pipe_index: u8,
        pipe_information: *mut WINUSB_PIPE_INFORMATION,
    ) -> BOOL;
    pub fn WinUsb_SetPipePolicy(
        interface_handle: WINUSB_INTERFACE_HANDLE,
        pipe_id: u8,
        policy_type: u32,
        value_length: u32,
        value: *const c_void,
    ) -> BOOL;
    pub fn WinUsb_GetPipePolicy(
        interface_handle: WINUSB_INTERFACE_HANDLE,
        pipe_id: u8,
        policy_type: u32,
        value_length: *mut u32,
        value: *mut c_void,
    ) -> BOOL;
    pub fn WinUsb_ReadPipe(
        interface_handle: WINUSB_INTERFACE_HANDLE,
        pipe_id: u8,
        buffer: *mut u8,
        buffer_length: u32,
        length_transferred: *mut u32,
        overlapped: *mut OVERLAPPED,
    ) -> BOOL;
    pub fn WinUsb_WritePipe(
        interface_handle: WINUSB_INTERFACE_HANDLE,
        pipe_id: u8,
        buffer: *const u8,
        buffer_length: u32,
        length_transferred: *mut u32,
        overlapped: *mut OVERLAPPED,
    ) -> BOOL;
    pub fn WinUsb_ControlTransfer(
        interface_handle: WINUSB_INTERFACE_HANDLE,
        setup_packet: WINUSB_SETUP_PACKET,
        buffer: *mut u8,
        buffer_length: u32,
        length_transferred: *mut u32,
        overlapped: *mut OVERLAPPED,
    ) -> BOOL;
    pub fn WinUsb_ResetPipe(
        interface_handle: WINUSB_INTERFACE_HANDLE,
        pipe_id: u8,
    ) -> BOOL;
    pub fn WinUsb_AbortPipe(
        interface_handle: WINUSB_INTERFACE_HANDLE,
        pipe_id: u8,
    ) -> BOOL;
    pub fn WinUsb_FlushPipe(
        interface_handle: WINUSB_INTERFACE_HANDLE,
        pipe_id: u8,
    ) -> BOOL;
}

#[cfg(target_os = "windows")]
#[link(name = "setupapi")]
extern "system" {
    pub fn SetupDiGetClassDevsW(
        class_guid: *const GUID,
        enumerator: *const u16,
        hwnd_parent: *mut c_void,
        flags: u32,
    ) -> HANDLE;
    pub fn SetupDiEnumDeviceInterfaces(
        device_info_set: HANDLE,
        device_info_data: *mut SP_DEVINFO_DATA,
        interface_class_guid: *const GUID,
        member_index: u32,
        device_interface_data: *mut SP_DEVICE_INTERFACE_DATA,
    ) -> BOOL;
    pub fn SetupDiGetDeviceInterfaceDetailW(
        device_info_set: HANDLE,
        device_interface_data: *mut SP_DEVICE_INTERFACE_DATA,
        device_interface_detail_data: *mut u8,
        device_interface_detail_data_size: u32,
        required_size: *mut u32,
        device_info_data: *mut SP_DEVINFO_DATA,
    ) -> BOOL;
    pub fn SetupDiDestroyDeviceInfoList(device_info_set: HANDLE) -> BOOL;
}

// Fallback stubs for non-Windows platforms (enables CI cross-check)
#[cfg(not(target_os = "windows"))]
pub unsafe fn WinUsb_Initialize(_: HANDLE, _: *mut WINUSB_INTERFACE_HANDLE) -> BOOL { 0 }
#[cfg(not(target_os = "windows"))]
pub unsafe fn WinUsb_Free(_: WINUSB_INTERFACE_HANDLE) -> BOOL { 1 }
#[cfg(not(target_os = "windows"))]
pub unsafe fn WinUsb_ControlTransfer(_: WINUSB_INTERFACE_HANDLE, _: WINUSB_SETUP_PACKET, _: *mut u8, _: u32, _: *mut u32, _: *mut OVERLAPPED) -> BOOL { 0 }
#[cfg(not(target_os = "windows"))]
pub unsafe fn WinUsb_ReadPipe(_: WINUSB_INTERFACE_HANDLE, _: u8, _: *mut u8, _: u32, _: *mut u32, _: *mut OVERLAPPED) -> BOOL { 0 }
#[cfg(not(target_os = "windows"))]
pub unsafe fn WinUsb_WritePipe(_: WINUSB_INTERFACE_HANDLE, _: u8, _: *const u8, _: u32, _: *mut u32, _: *mut OVERLAPPED) -> BOOL { 0 }
#[cfg(not(target_os = "windows"))]
pub unsafe fn WinUsb_ResetPipe(_: WINUSB_INTERFACE_HANDLE, _: u8) -> BOOL { 1 }
#[cfg(not(target_os = "windows"))]
pub unsafe fn WinUsb_AbortPipe(_: WINUSB_INTERFACE_HANDLE, _: u8) -> BOOL { 1 }
#[cfg(not(target_os = "windows"))]
pub unsafe fn WinUsb_SetPipePolicy(_: WINUSB_INTERFACE_HANDLE, _: u8, _: u32, _: u32, _: *const c_void) -> BOOL { 1 }

