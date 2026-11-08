//! Safe, native Rust wrapper for WinUSB Device and Pipe Operations
//! Provides synchronous and overlapped Control, Bulk IN, and Bulk OUT operations
//! with strict timeouts, stall recovery, and pipe abort capabilities.

use super::ffi::*;
use std::ffi::c_void;
use std::sync::Mutex;
use thiserror::Error;

pub const DEFAULT_USB_TIMEOUT_MS: u32 = 3000;
pub const ERROR_IO_PENDING: u32 = 997;
pub const ERROR_DEVICE_NOT_CONNECTED: u32 = 1167;
pub const ERROR_GEN_FAILURE: u32 = 31;
pub const ERROR_OPERATION_ABORTED: u32 = 995;

#[derive(Debug, Error)]
pub enum WinUsbError {
    #[error("Failed to open device path '{path}': Windows error {error_code}")]
    DeviceOpenFailed { path: String, error_code: u32 },

    #[error("WinUsb_Initialize failed: Windows error {0}")]
    WinUsbInitFailed(u32),

    #[error("I/O failed on endpoint 0x{endpoint:02X} ({op}): Windows error {error_code}")]
    IoFailed { op: String, endpoint: u8, error_code: u32 },

    #[error("USB transfer timed out on endpoint 0x{endpoint:02X} after {timeout_ms} ms")]
    Timeout { endpoint: u8, timeout_ms: u32 },

    #[error("Endpoint 0x{0:02X} stalled (pipe halt)")]
    Stalled(u8),

    #[error("Physical device disconnected or unplugged")]
    DeviceDisconnected,

    #[error("Device operation aborted / cancelled")]
    Aborted,

    #[error("Invalid parameter: {0}")]
    InvalidParameter(String),

    #[error("Pipe 0x{0:02X} not found on interface")]
    PipeNotFound(u8),
}

#[derive(Debug, Clone)]
pub struct UsbPipeInfo {
    pub pipe_id: u8,
    pub pipe_type: u32,
    pub max_packet_size: u16,
    pub interval: u8,
}

pub struct WinUsbDevice {
    pub path: String,
    file_handle: HANDLE,
    winusb_handle: WINUSB_INTERFACE_HANDLE,
    pub pipes: Vec<UsbPipeInfo>,
    io_lock: Mutex<()>,
}

unsafe impl Send for WinUsbDevice {}
unsafe impl Sync for WinUsbDevice {}

impl WinUsbDevice {
    /// Opens a physical USB device via its WinUSB device interface path and initializes WinUSB.
    pub fn open(path: &str) -> Result<Self, WinUsbError> {
        #[cfg(not(target_os = "windows"))]
        {
            Ok(Self {
                path: path.to_string(),
                file_handle: INVALID_HANDLE_VALUE,
                winusb_handle: std::ptr::null_mut(),
                pipes: Vec::new(),
                io_lock: Mutex::new(()),
            })
        }

        #[cfg(target_os = "windows")]
        unsafe {
            let wide_path: Vec<u16> = path.encode_utf16().chain(std::iter::once(0)).collect();
            let file_handle = CreateFileW(
                wide_path.as_ptr(),
                GENERIC_READ | GENERIC_WRITE,
                FILE_SHARE_READ | FILE_SHARE_WRITE,
                std::ptr::null_mut(),
                OPEN_EXISTING,
                FILE_ATTRIBUTE_NORMAL | FILE_FLAG_OVERLAPPED,
                std::ptr::null_mut(),
            );

            if file_handle == INVALID_HANDLE_VALUE || file_handle.is_null() {
                let err = GetLastError();
                return Err(WinUsbError::DeviceOpenFailed {
                    path: path.to_string(),
                    error_code: err,
                });
            }

            let mut winusb_handle: WINUSB_INTERFACE_HANDLE = std::ptr::null_mut();
            if WinUsb_Initialize(file_handle, &mut winusb_handle) == 0 {
                let err = GetLastError();
                CloseHandle(file_handle);
                return Err(WinUsbError::WinUsbInitFailed(err));
            }

            // Enumerate and query pipes on alternate interface 0
            let mut pipes = Vec::new();
            let mut pipe_idx = 0u8;
            loop {
                let mut pipe_info = WINUSB_PIPE_INFORMATION::default();
                if WinUsb_QueryPipe(winusb_handle, 0, pipe_idx, &mut pipe_info) == 0 {
                    break;
                }
                pipes.push(UsbPipeInfo {
                    pipe_id: pipe_info.pipe_id,
                    pipe_type: pipe_info.pipe_type,
                    max_packet_size: pipe_info.maximum_packet_size,
                    interval: pipe_info.interval,
                });

                // Configure Pipe Policies:
                // Auto clear stall on bulk endpoints
                let auto_clear_stall: u8 = 1;
                WinUsb_SetPipePolicy(
                    winusb_handle,
                    pipe_info.pipe_id,
                    AUTO_CLEAR_STALL,
                    std::mem::size_of::<u8>() as u32,
                    &auto_clear_stall as *const u8 as *const c_void,
                );

                // Set pipe transfer timeout to 3000 ms
                let timeout_ms: u32 = DEFAULT_USB_TIMEOUT_MS;
                WinUsb_SetPipePolicy(
                    winusb_handle,
                    pipe_info.pipe_id,
                    PIPE_TRANSFER_TIMEOUT,
                    std::mem::size_of::<u32>() as u32,
                    &timeout_ms as *const u32 as *const c_void,
                );

                pipe_idx += 1;
            }

            Ok(Self {
                path: path.to_string(),
                file_handle,
                winusb_handle,
                pipes,
                io_lock: Mutex::new(()),
            })
        }
    }

    /// Dispatches a Control Transfer (EP0) using standard 8-byte USB Setup Packet.
    pub fn control_transfer(
        &self,
        setup_packet_bytes: &[u8; 8],
        buffer: &mut [u8],
        timeout_ms: u32,
    ) -> Result<u32, WinUsbError> {
        #[cfg(not(target_os = "windows"))]
        {
            // Cross-platform mock return for unit tests
            let length = u16::from_le_bytes([setup_packet_bytes[6], setup_packet_bytes[7]]) as usize;
            let copy_len = length.min(buffer.len());
            Ok(copy_len as u32)
        }

        #[cfg(target_os = "windows")]
        unsafe {
            let _lock = self.io_lock.lock().unwrap();
            let sp = WINUSB_SETUP_PACKET {
                request_type: setup_packet_bytes[0],
                request: setup_packet_bytes[1],
                value: u16::from_le_bytes([setup_packet_bytes[2], setup_packet_bytes[3]]),
                index: u16::from_le_bytes([setup_packet_bytes[4], setup_packet_bytes[5]]),
                length: u16::from_le_bytes([setup_packet_bytes[6], setup_packet_bytes[7]]),
            };

            let h_event = CreateEventW(std::ptr::null_mut(), 1, 0, std::ptr::null());
            if h_event.is_null() {
                return Err(WinUsbError::IoFailed {
                    op: "CreateEvent".to_string(),
                    endpoint: 0,
                    error_code: GetLastError(),
                });
            }

            let mut ov = OVERLAPPED {
                h_event,
                ..Default::default()
            };

            let mut transferred = 0u32;
            let buf_ptr = if buffer.is_empty() {
                std::ptr::null_mut()
            } else {
                buffer.as_mut_ptr()
            };
            let buf_len = buffer.len() as u32;

            let res = WinUsb_ControlTransfer(
                self.winusb_handle,
                sp,
                buf_ptr,
                buf_len,
                &mut transferred,
                &mut ov,
            );

            let final_res = if res != 0 {
                Ok(transferred)
            } else {
                let err = GetLastError();
                if err == ERROR_IO_PENDING {
                    let wait = WaitForSingleObject(h_event, timeout_ms);
                    if wait == WAIT_OBJECT_0 {
                        if GetOverlappedResult(self.file_handle, &mut ov, &mut transferred, 0) != 0 {
                            Ok(transferred)
                        } else {
                            Err(self.map_win32_error("ControlTransfer", 0, GetLastError()))
                        }
                    } else if wait == WAIT_TIMEOUT {
                        CancelIoEx(self.file_handle, &mut ov);
                        let mut transferred_dummy = 0u32;
                        let _ = GetOverlappedResult(self.file_handle, &mut ov, &mut transferred_dummy, 1);
                        Err(WinUsbError::Timeout { endpoint: 0, timeout_ms })
                    } else {
                        Err(WinUsbError::IoFailed {
                            op: "WaitForSingleObject".to_string(),
                            endpoint: 0,
                            error_code: GetLastError(),
                        })
                    }
                } else {
                    Err(self.map_win32_error("ControlTransfer", 0, err))
                }
            };

            CloseHandle(h_event);
            final_res
        }
    }

    /// Reads data from a Bulk or Interrupt IN endpoint.
    pub fn read_pipe(
        &self,
        pipe_id: u8,
        buffer: &mut [u8],
        timeout_ms: u32,
    ) -> Result<u32, WinUsbError> {
        #[cfg(not(target_os = "windows"))]
        {
            Ok(0)
        }

        #[cfg(target_os = "windows")]
        unsafe {
            let _lock = self.io_lock.lock().unwrap();
            let h_event = CreateEventW(std::ptr::null_mut(), 1, 0, std::ptr::null());
            if h_event.is_null() {
                return Err(WinUsbError::IoFailed {
                    op: "CreateEvent".to_string(),
                    endpoint: pipe_id,
                    error_code: GetLastError(),
                });
            }

            let mut ov = OVERLAPPED {
                h_event,
                ..Default::default()
            };

            let mut transferred = 0u32;
            let res = WinUsb_ReadPipe(
                self.winusb_handle,
                pipe_id,
                buffer.as_mut_ptr(),
                buffer.len() as u32,
                &mut transferred,
                &mut ov,
            );

            let final_res = if res != 0 {
                Ok(transferred)
            } else {
                let err = GetLastError();
                if err == ERROR_IO_PENDING {
                    let wait = WaitForSingleObject(h_event, timeout_ms);
                    if wait == WAIT_OBJECT_0 {
                        if GetOverlappedResult(self.file_handle, &mut ov, &mut transferred, 0) != 0 {
                            Ok(transferred)
                        } else {
                            Err(self.map_win32_error("ReadPipe", pipe_id, GetLastError()))
                        }
                    } else if wait == WAIT_TIMEOUT {
                        CancelIoEx(self.file_handle, &mut ov);
                        let _ = WinUsb_AbortPipe(self.winusb_handle, pipe_id);
                        let mut transferred_dummy = 0u32;
                        let _ = GetOverlappedResult(self.file_handle, &mut ov, &mut transferred_dummy, 1);
                        Err(WinUsbError::Timeout { endpoint: pipe_id, timeout_ms })
                    } else {
                        Err(WinUsbError::IoFailed {
                            op: "WaitForSingleObject".to_string(),
                            endpoint: pipe_id,
                            error_code: GetLastError(),
                        })
                    }
                } else {
                    Err(self.map_win32_error("ReadPipe", pipe_id, err))
                }
            };

            CloseHandle(h_event);
            final_res
        }
    }

    /// Writes data to a Bulk or Interrupt OUT endpoint.
    pub fn write_pipe(
        &self,
        pipe_id: u8,
        data: &[u8],
        timeout_ms: u32,
    ) -> Result<u32, WinUsbError> {
        #[cfg(not(target_os = "windows"))]
        {
            Ok(data.len() as u32)
        }

        #[cfg(target_os = "windows")]
        unsafe {
            let _lock = self.io_lock.lock().unwrap();
            let h_event = CreateEventW(std::ptr::null_mut(), 1, 0, std::ptr::null());
            if h_event.is_null() {
                return Err(WinUsbError::IoFailed {
                    op: "CreateEvent".to_string(),
                    endpoint: pipe_id,
                    error_code: GetLastError(),
                });
            }

            let mut ov = OVERLAPPED {
                h_event,
                ..Default::default()
            };

            let mut transferred = 0u32;
            let res = WinUsb_WritePipe(
                self.winusb_handle,
                pipe_id,
                data.as_ptr(),
                data.len() as u32,
                &mut transferred,
                &mut ov,
            );

            let final_res = if res != 0 {
                Ok(transferred)
            } else {
                let err = GetLastError();
                if err == ERROR_IO_PENDING {
                    let wait = WaitForSingleObject(h_event, timeout_ms);
                    if wait == WAIT_OBJECT_0 {
                        if GetOverlappedResult(self.file_handle, &mut ov, &mut transferred, 0) != 0 {
                            Ok(transferred)
                        } else {
                            Err(self.map_win32_error("WritePipe", pipe_id, GetLastError()))
                        }
                    } else if wait == WAIT_TIMEOUT {
                        CancelIoEx(self.file_handle, &mut ov);
                        let _ = WinUsb_AbortPipe(self.winusb_handle, pipe_id);
                        let mut transferred_dummy = 0u32;
                        let _ = GetOverlappedResult(self.file_handle, &mut ov, &mut transferred_dummy, 1);
                        Err(WinUsbError::Timeout { endpoint: pipe_id, timeout_ms })
                    } else {
                        Err(WinUsbError::IoFailed {
                            op: "WaitForSingleObject".to_string(),
                            endpoint: pipe_id,
                            error_code: GetLastError(),
                        })
                    }
                } else {
                    Err(self.map_win32_error("WritePipe", pipe_id, err))
                }
            };

            CloseHandle(h_event);
            final_res
        }
    }

    /// Cancels all pending transfers on a pipe (WinUsb_AbortPipe).
    pub fn abort_pipe(&self, pipe_id: u8) -> Result<(), WinUsbError> {
        #[cfg(not(target_os = "windows"))]
        {
            Ok(())
        }

        #[cfg(target_os = "windows")]
        unsafe {
            if WinUsb_AbortPipe(self.winusb_handle, pipe_id) != 0 {
                Ok(())
            } else {
                Err(WinUsbError::IoFailed {
                    op: "AbortPipe".to_string(),
                    endpoint: pipe_id,
                    error_code: GetLastError(),
                })
            }
        }
    }

    /// Resets a pipe and clears its halt/stall status (WinUsb_ResetPipe).
    pub fn reset_pipe(&self, pipe_id: u8) -> Result<(), WinUsbError> {
        #[cfg(not(target_os = "windows"))]
        {
            Ok(())
        }

        #[cfg(target_os = "windows")]
        unsafe {
            if WinUsb_ResetPipe(self.winusb_handle, pipe_id) != 0 {
                Ok(())
            } else {
                Err(WinUsbError::IoFailed {
                    op: "ResetPipe".to_string(),
                    endpoint: pipe_id,
                    error_code: GetLastError(),
                })
            }
        }
    }

    #[cfg(target_os = "windows")]
    fn map_win32_error(&self, op: &str, endpoint: u8, code: u32) -> WinUsbError {
        match code {
            ERROR_DEVICE_NOT_CONNECTED => WinUsbError::DeviceDisconnected,
            ERROR_OPERATION_ABORTED => WinUsbError::Aborted,
            ERROR_GEN_FAILURE => WinUsbError::Stalled(endpoint),
            _ => WinUsbError::IoFailed {
                op: op.to_string(),
                endpoint,
                error_code: code,
            },
        }
    }
}

impl Drop for WinUsbDevice {
    fn drop(&mut self) {
        #[cfg(target_os = "windows")]
        unsafe {
            if !self.winusb_handle.is_null() {
                WinUsb_Free(self.winusb_handle);
                self.winusb_handle = std::ptr::null_mut();
            }
            if self.file_handle != INVALID_HANDLE_VALUE && !self.file_handle.is_null() {
                CloseHandle(self.file_handle);
                self.file_handle = INVALID_HANDLE_VALUE;
            }
        }
    }
}

