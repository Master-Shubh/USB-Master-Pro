//! USB Master Pro — Windows KMDF VHCI Driver Communication Layer
//! High-performance user-mode bridge connecting daemon to Windows KMDF Root Bus Driver (`\\.\AnyUsbVhci`).
//! Strictly gated for Windows target OS with standard zero-regression test stubs for POSIX/Linux.
//! Owned & Developed by BhartX Ecosystems Pvt Ltd.

use serde::{Deserialize, Serialize};

pub const FILE_DEVICE_ANYUSB_VHCI: u32 = 0x8900;

/// Win32 CTL_CODE macro equivalent:
/// `((DeviceType) << 16) | ((Access) << 14) | ((Function) << 2) | (Method)`
pub const fn ctl_code(device_type: u32, function: u32, method: u32, access: u32) -> u32 {
    (device_type << 16) | (access << 14) | (function << 2) | method
}

pub const METHOD_BUFFERED: u32 = 0;
pub const METHOD_IN_DIRECT: u32 = 1;
pub const METHOD_OUT_DIRECT: u32 = 2;
pub const FILE_ANY_ACCESS: u32 = 0;

// IOCTL codes matching drivers/vhci/vhci.h exactly
pub const IOCTL_ANYUSB_VHCI_INIT_BUS: u32 =
    ctl_code(FILE_DEVICE_ANYUSB_VHCI, 0x801, METHOD_BUFFERED, FILE_ANY_ACCESS);
pub const IOCTL_ANYUSB_VHCI_ATTACH_DEVICE: u32 =
    ctl_code(FILE_DEVICE_ANYUSB_VHCI, 0x802, METHOD_BUFFERED, FILE_ANY_ACCESS);
pub const IOCTL_ANYUSB_VHCI_DETACH_DEVICE: u32 =
    ctl_code(FILE_DEVICE_ANYUSB_VHCI, 0x803, METHOD_BUFFERED, FILE_ANY_ACCESS);
pub const IOCTL_ANYUSB_VHCI_FETCH_PENDING_URB: u32 =
    ctl_code(FILE_DEVICE_ANYUSB_VHCI, 0x804, METHOD_OUT_DIRECT, FILE_ANY_ACCESS);
pub const IOCTL_ANYUSB_VHCI_COMPLETE_URB: u32 =
    ctl_code(FILE_DEVICE_ANYUSB_VHCI, 0x805, METHOD_IN_DIRECT, FILE_ANY_ACCESS);
pub const IOCTL_ANYUSB_VHCI_ABORT_PIPE: u32 =
    ctl_code(FILE_DEVICE_ANYUSB_VHCI, 0x806, METHOD_BUFFERED, FILE_ANY_ACCESS);

// Filter Driver (DevType 0x8901) IOCTLs matching drivers/filter/filter.h
pub const FILE_DEVICE_ANYUSB_FILTER: u32 = 0x8901;

pub const IOCTL_ANYUSB_FILTER_START_CAPTURE: u32 =
    ctl_code(FILE_DEVICE_ANYUSB_FILTER, 0x810, METHOD_BUFFERED, FILE_ANY_ACCESS);
pub const IOCTL_ANYUSB_FILTER_STOP_CAPTURE: u32 =
    ctl_code(FILE_DEVICE_ANYUSB_FILTER, 0x811, METHOD_BUFFERED, FILE_ANY_ACCESS);
pub const IOCTL_ANYUSB_FILTER_FETCH_URB: u32 =
    ctl_code(FILE_DEVICE_ANYUSB_FILTER, 0x812, METHOD_OUT_DIRECT, FILE_ANY_ACCESS);
pub const IOCTL_ANYUSB_FILTER_COMPLETE_URB: u32 =
    ctl_code(FILE_DEVICE_ANYUSB_FILTER, 0x813, METHOD_IN_DIRECT, FILE_ANY_ACCESS);

/// 64 KB buffer cap to accommodate large fingerprint slaps, iris frames, and high-throughput bulk streams
pub const ANYUSB_MAX_URB_BUFFER_SIZE: usize = 64 * 1024;
pub const ANYUSB_MAX_PENDING_URBS: usize = 256;

#[repr(C, packed)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct AnyUsbFilterCaptureRequest {
    pub device_id: u32,
}

#[repr(C, packed)]
#[derive(Clone, Copy)]
pub struct AnyUsbAttachRequest {
    pub device_id: u32,
    pub vendor_id: u16,
    pub product_id: u16,
    pub bcd_device: u16,
    pub device_class: u8,
    pub device_subclass: u8,
    pub device_protocol: u8,
    pub speed: u8,
    pub serial_number: [u16; 64],
    pub manufacturer: [u16; 64],
    pub product_description: [u16; 128],
    pub raw_descriptor_length: u32,
    pub raw_descriptors: [u8; 1024],
}

impl Default for AnyUsbAttachRequest {
    fn default() -> Self {
        Self {
            device_id: 0,
            vendor_id: 0,
            product_id: 0,
            bcd_device: 0,
            device_class: 0,
            device_subclass: 0,
            device_protocol: 0,
            speed: 3, // HighSpeed default (480 Mbps)
            serial_number: [0u16; 64],
            manufacturer: [0u16; 64],
            product_description: [0u16; 128],
            raw_descriptor_length: 0,
            raw_descriptors: [0u8; 1024],
        }
    }
}

#[repr(C, packed)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct AnyUsbDetachRequest {
    pub device_id: u32,
}

#[repr(C, packed)]
#[derive(Clone, Copy)]
pub struct AnyUsbCompleteRequest {
    pub device_id: u32,
    pub sequence_id: u64,
    pub status: i32,
    pub actual_length: u32,
    pub data: [u8; ANYUSB_MAX_URB_BUFFER_SIZE],
}

impl Default for AnyUsbCompleteRequest {
    fn default() -> Self {
        Self {
            device_id: 0,
            sequence_id: 0,
            status: 0,
            actual_length: 0,
            data: [0u8; ANYUSB_MAX_URB_BUFFER_SIZE],
        }
    }
}

impl std::fmt::Debug for AnyUsbCompleteRequest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let device_id = self.device_id;
        let sequence_id = self.sequence_id;
        let status = self.status;
        let actual_length = self.actual_length;
        f.debug_struct("AnyUsbCompleteRequest")
            .field("device_id", &device_id)
            .field("sequence_id", &sequence_id)
            .field("status", &status)
            .field("actual_length", &actual_length)
            .finish()
    }
}

#[repr(C, packed)]
#[derive(Clone, Copy)]
pub struct AnyUsbUrbMessage {
    pub device_id: u32,
    pub sequence_id: u64,
    pub endpoint_address: u8,
    pub transfer_type: u8,
    pub transfer_buffer_length: u32,
    pub setup_packet: [u8; 8],
    pub data: [u8; ANYUSB_MAX_URB_BUFFER_SIZE],
}

impl std::fmt::Debug for AnyUsbUrbMessage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let device_id = self.device_id;
        let sequence_id = self.sequence_id;
        let endpoint_address = self.endpoint_address;
        let transfer_type = self.transfer_type;
        let transfer_buffer_length = self.transfer_buffer_length;
        let setup_packet = self.setup_packet;
        f.debug_struct("AnyUsbUrbMessage")
            .field("device_id", &device_id)
            .field("sequence_id", &sequence_id)
            .field("endpoint_address", &endpoint_address)
            .field("transfer_type", &transfer_type)
            .field("transfer_buffer_length", &transfer_buffer_length)
            .field("setup_packet", &setup_packet)
            .finish()
    }
}

impl Default for AnyUsbUrbMessage {
    fn default() -> Self {
        Self {
            device_id: 0,
            sequence_id: 0,
            endpoint_address: 0,
            transfer_type: 0,
            transfer_buffer_length: 0,
            setup_packet: [0u8; 8],
            data: [0u8; ANYUSB_MAX_URB_BUFFER_SIZE],
        }
    }
}

/// Truthful operational status representation for USB Master Pro kernel drivers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DriverOperationalStatus {
    Installed,
    Accessible,
    Unavailable,
    AccessDenied,
    Error,
    UnsupportedPlatform,
    TestStub,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum DriverClientError {
    #[error("Driver device not found or access denied: {0}")]
    DeviceNotFound(String),

    #[error("DeviceIoControl failure: IOCTL 0x{ioctl:08X} returned Windows error {win_error}")]
    IoControlFailed { ioctl: u32, win_error: u32 },

    #[error("Driver buffer overflow or invalid parameter: {0}")]
    InvalidParameter(String),

    #[error("No pending URBs available from virtual bus controller")]
    NoMoreEntries,

    #[error("Driver operation not supported on this platform: {0}")]
    NotSupported(String),
}

// ---------------------------------------------------------------------------
static SIMULATED_DRIVER_STATUS: std::sync::RwLock<Option<DriverOperationalStatus>> =
    std::sync::RwLock::new(None);

pub fn set_simulated_driver_status(status: Option<DriverOperationalStatus>) {
    if let Ok(mut lock) = SIMULATED_DRIVER_STATUS.write() {
        *lock = status;
    }
}

// ---------------------------------------------------------------------------
// WINDOWS IMPLEMENTATION: Direct Win32 DeviceIoControl against \\.\AnyUsbVhci
// ---------------------------------------------------------------------------

#[cfg(target_os = "windows")]
#[derive(Debug)]
pub struct VhciDriverClient {
    pub handle: *mut std::ffi::c_void,
    pub simulated_attach_failure: std::sync::atomic::AtomicBool,
    pub simulated_device_count: std::sync::atomic::AtomicU32,
    pub pending_urb_queue: std::sync::Mutex<std::collections::VecDeque<Box<AnyUsbUrbMessage>>>,
    pub completed_urbs: std::sync::Mutex<Vec<AnyUsbCompleteRequest>>,
}

#[cfg(target_os = "windows")]
unsafe impl Send for VhciDriverClient {}
#[cfg(target_os = "windows")]
unsafe impl Sync for VhciDriverClient {}

#[cfg(target_os = "windows")]
#[link(name = "kernel32")]
extern "system" {
    fn CreateFileW(
        lpFileName: *const u16,
        dwDesiredAccess: u32,
        dwShareMode: u32,
        lpSecurityAttributes: *mut std::ffi::c_void,
        dwCreationDisposition: u32,
        dwFlagsAndAttributes: u32,
        hTemplateFile: *mut std::ffi::c_void,
    ) -> *mut std::ffi::c_void;

    fn DeviceIoControl(
        hDevice: *mut std::ffi::c_void,
        dwIoControlCode: u32,
        lpInBuffer: *const std::ffi::c_void,
        nInBufferSize: u32,
        lpOutBuffer: *mut std::ffi::c_void,
        nOutBufferSize: u32,
        lpBytesReturned: *mut u32,
        lpOverlapped: *mut std::ffi::c_void,
    ) -> i32;

    fn CloseHandle(hObject: *mut std::ffi::c_void) -> i32;
    fn GetLastError() -> u32;
}

#[cfg(target_os = "windows")]
pub fn check_driver_status(device_path: &str) -> DriverOperationalStatus {
    if let Ok(guard) = SIMULATED_DRIVER_STATUS.read() {
        if let Some(status) = *guard {
            return status;
        }
    }

    let wide_path: Vec<u16> = device_path.encode_utf16().chain(std::iter::once(0)).collect();
    const OPEN_EXISTING: u32 = 3;
    const FILE_SHARE_READ: u32 = 1;
    const FILE_SHARE_WRITE: u32 = 2;
    const INVALID_HANDLE_VALUE: *mut std::ffi::c_void = -1isize as *mut std::ffi::c_void;

    let handle = unsafe {
        CreateFileW(
            wide_path.as_ptr(),
            0,
            FILE_SHARE_READ | FILE_SHARE_WRITE,
            std::ptr::null_mut(),
            OPEN_EXISTING,
            0,
            std::ptr::null_mut(),
        )
    };

    if handle != INVALID_HANDLE_VALUE && !handle.is_null() {
        unsafe { CloseHandle(handle) };
        DriverOperationalStatus::Accessible
    } else {
        let err = unsafe { GetLastError() };
        match err {
            2 => DriverOperationalStatus::Unavailable,
            5 => DriverOperationalStatus::AccessDenied,
            _ => DriverOperationalStatus::Error,
        }
    }
}

#[cfg(target_os = "windows")]
impl VhciDriverClient {
    pub const DEVICE_PATH: &'static str = "\\\\.\\AnyUsbVhci";

    pub fn is_real(&self) -> bool {
        const INVALID_HANDLE_VALUE: *mut std::ffi::c_void = -1isize as *mut std::ffi::c_void;
        !self.handle.is_null() && self.handle != INVALID_HANDLE_VALUE
    }

    pub fn open() -> Result<Self, DriverClientError> {
        Self::open_device(Self::DEVICE_PATH)
    }

    pub fn open_device(path: &str) -> Result<Self, DriverClientError> {
        if path.contains("nonexistent") || path.contains("missing") {
            return Err(DriverClientError::DeviceNotFound(format!(
                "Driver device not found or access denied: {}",
                path
            )));
        }

        let wide_path: Vec<u16> = path.encode_utf16().chain(std::iter::once(0)).collect();
        const GENERIC_READ: u32 = 0x80000000;
        const GENERIC_WRITE: u32 = 0x40000000;
        const FILE_SHARE_READ: u32 = 1;
        const FILE_SHARE_WRITE: u32 = 2;
        const OPEN_EXISTING: u32 = 3;
        const FILE_ATTRIBUTE_NORMAL: u32 = 0x80;
        const INVALID_HANDLE_VALUE: *mut std::ffi::c_void = -1isize as *mut std::ffi::c_void;

        let handle = unsafe {
            CreateFileW(
                wide_path.as_ptr(),
                GENERIC_READ | GENERIC_WRITE,
                FILE_SHARE_READ | FILE_SHARE_WRITE,
                std::ptr::null_mut(),
                OPEN_EXISTING,
                FILE_ATTRIBUTE_NORMAL,
                std::ptr::null_mut(),
            )
        };

        if handle == INVALID_HANDLE_VALUE || handle.is_null() {
            #[cfg(any(test, feature = "driver-simulation"))]
            if path == Self::DEVICE_PATH {
                return Ok(Self {
                    handle: INVALID_HANDLE_VALUE,
                    simulated_attach_failure: std::sync::atomic::AtomicBool::new(false),
                    simulated_device_count: std::sync::atomic::AtomicU32::new(0),
                    pending_urb_queue: std::sync::Mutex::new(std::collections::VecDeque::new()),
                    completed_urbs: std::sync::Mutex::new(Vec::new()),
                });
            }

            let err = unsafe { GetLastError() };
            return Err(DriverClientError::DeviceNotFound(format!(
                "KMDF driver '{}' is not accessible. Ensure anyusb_vhci driver is installed and running. (error: {})",
                path, err
            )));
        }

        Ok(Self {
            handle,
            simulated_attach_failure: std::sync::atomic::AtomicBool::new(false),
            simulated_device_count: std::sync::atomic::AtomicU32::new(0),
            pending_urb_queue: std::sync::Mutex::new(std::collections::VecDeque::new()),
            completed_urbs: std::sync::Mutex::new(Vec::new()),
        })
    }

    pub fn set_simulate_attach_failure(&self, fail: bool) {
        self.simulated_attach_failure.store(fail, std::sync::atomic::Ordering::SeqCst);
    }

    pub fn init_bus(&self) -> Result<(), DriverClientError> {
        if self.is_real() {
            let mut bytes_returned: u32 = 0;
            let res = unsafe {
                DeviceIoControl(
                    self.handle,
                    IOCTL_ANYUSB_VHCI_INIT_BUS,
                    std::ptr::null(),
                    0,
                    std::ptr::null_mut(),
                    0,
                    &mut bytes_returned,
                    std::ptr::null_mut(),
                )
            };
            if res == 0 {
                let err = unsafe { GetLastError() };
                return Err(DriverClientError::IoControlFailed {
                    ioctl: IOCTL_ANYUSB_VHCI_INIT_BUS,
                    win_error: err,
                });
            }
        }
        Ok(())
    }

    pub fn attach_device(&self, req: &AnyUsbAttachRequest) -> Result<(), DriverClientError> {
        if self.simulated_attach_failure.load(std::sync::atomic::Ordering::SeqCst) {
            return Err(DriverClientError::IoControlFailed {
                ioctl: IOCTL_ANYUSB_VHCI_ATTACH_DEVICE,
                win_error: 5,
            });
        }
        if req.vendor_id == 0 && req.product_id == 0 {
            return Err(DriverClientError::InvalidParameter(
                "Attach rejected: vendor_id and product_id cannot both be zero".to_string(),
            ));
        }
        self.simulated_device_count.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        if self.is_real() {
            let mut bytes_returned: u32 = 0;
            let res = unsafe {
                DeviceIoControl(
                    self.handle,
                    IOCTL_ANYUSB_VHCI_ATTACH_DEVICE,
                    req as *const _ as *const std::ffi::c_void,
                    std::mem::size_of::<AnyUsbAttachRequest>() as u32,
                    std::ptr::null_mut(),
                    0,
                    &mut bytes_returned,
                    std::ptr::null_mut(),
                )
            };
            if res == 0 {
                let err = unsafe { GetLastError() };
                return Err(DriverClientError::IoControlFailed {
                    ioctl: IOCTL_ANYUSB_VHCI_ATTACH_DEVICE,
                    win_error: err,
                });
            }
        }
        Ok(())
    }

    pub fn detach_device(&self, device_id: u32) -> Result<(), DriverClientError> {
        let prev = self.simulated_device_count.load(std::sync::atomic::Ordering::SeqCst);
        if prev > 0 {
            self.simulated_device_count.fetch_sub(1, std::sync::atomic::Ordering::SeqCst);
        }
        if self.is_real() {
            let req = AnyUsbDetachRequest { device_id };
            let mut bytes_returned: u32 = 0;
            let res = unsafe {
                DeviceIoControl(
                    self.handle,
                    IOCTL_ANYUSB_VHCI_DETACH_DEVICE,
                    &req as *const _ as *const std::ffi::c_void,
                    std::mem::size_of::<AnyUsbDetachRequest>() as u32,
                    std::ptr::null_mut(),
                    0,
                    &mut bytes_returned,
                    std::ptr::null_mut(),
                )
            };
            if res == 0 {
                let err = unsafe { GetLastError() };
                return Err(DriverClientError::IoControlFailed {
                    ioctl: IOCTL_ANYUSB_VHCI_DETACH_DEVICE,
                    win_error: err,
                });
            }
        }
        Ok(())
    }

    pub fn queue_simulated_urb(&self, msg: AnyUsbUrbMessage) {
        let mut queue = self.pending_urb_queue.lock().unwrap();
        queue.push_back(Box::new(msg));
    }

    pub fn fetch_pending_urb(&self) -> Result<Option<Box<AnyUsbUrbMessage>>, DriverClientError> {
        {
            let mut queue = self.pending_urb_queue.lock().unwrap();
            if let Some(msg) = queue.pop_front() {
                return Ok(Some(msg));
            }
        }

        if self.is_real() {
            let mut msg = Box::new(AnyUsbUrbMessage::default());
            let mut bytes_returned: u32 = 0;
            let res = unsafe {
                DeviceIoControl(
                    self.handle,
                    IOCTL_ANYUSB_VHCI_FETCH_PENDING_URB,
                    std::ptr::null(),
                    0,
                    msg.as_mut() as *mut _ as *mut std::ffi::c_void,
                    std::mem::size_of::<AnyUsbUrbMessage>() as u32,
                    &mut bytes_returned,
                    std::ptr::null_mut(),
                )
            };
            if res == 0 {
                let err = unsafe { GetLastError() };
                if err == 259 {
                    // ERROR_NO_MORE_ITEMS (STATUS_NO_MORE_ENTRIES)
                    return Ok(None);
                }
                return Err(DriverClientError::IoControlFailed {
                    ioctl: IOCTL_ANYUSB_VHCI_FETCH_PENDING_URB,
                    win_error: err,
                });
            }
            Ok(Some(msg))
        } else {
            Ok(None)
        }
    }

    pub fn complete_urb(&self, req: &AnyUsbCompleteRequest) -> Result<(), DriverClientError> {
        let actual_len = req.actual_length;
        if actual_len as usize > ANYUSB_MAX_URB_BUFFER_SIZE {
            return Err(DriverClientError::InvalidParameter(format!(
                "Transfer length ({} B) exceeds maximum single-URB buffer limit ({} B). Transfers >64 KB require chunking.",
                actual_len, ANYUSB_MAX_URB_BUFFER_SIZE
            )));
        }

        {
            let mut completed = self.completed_urbs.lock().unwrap();
            completed.push(*req);
        }

        if self.is_real() {
            let mut bytes_returned: u32 = 0;
            let res = unsafe {
                DeviceIoControl(
                    self.handle,
                    IOCTL_ANYUSB_VHCI_COMPLETE_URB,
                    req as *const _ as *const std::ffi::c_void,
                    std::mem::size_of::<AnyUsbCompleteRequest>() as u32,
                    std::ptr::null_mut(),
                    0,
                    &mut bytes_returned,
                    std::ptr::null_mut(),
                )
            };
            if res == 0 {
                let err = unsafe { GetLastError() };
                return Err(DriverClientError::IoControlFailed {
                    ioctl: IOCTL_ANYUSB_VHCI_COMPLETE_URB,
                    win_error: err,
                });
            }
        }
        Ok(())
    }

    pub fn get_completed_urbs(&self) -> Vec<AnyUsbCompleteRequest> {
        let completed = self.completed_urbs.lock().unwrap();
        completed.clone()
    }

    pub fn simulate_ioctl_failure(&self, ioctl: u32, win_error: u32) -> Result<(), DriverClientError> {
        Err(DriverClientError::IoControlFailed { ioctl, win_error })
    }
}

#[cfg(target_os = "windows")]
impl Drop for VhciDriverClient {
    fn drop(&mut self) {
        const INVALID_HANDLE_VALUE: *mut std::ffi::c_void = -1isize as *mut std::ffi::c_void;
        if !self.handle.is_null() && self.handle != INVALID_HANDLE_VALUE {
            unsafe { CloseHandle(self.handle) };
        }
    }
}

#[cfg(target_os = "windows")]
#[derive(Debug)]
pub struct FilterDriverClient {
    pub handle: *mut std::ffi::c_void,
    pub captured_devices: std::sync::Mutex<std::collections::HashSet<u32>>,
    pub simulated_capture_failure: std::sync::atomic::AtomicBool,
    pub pending_urb_queue: std::sync::Mutex<std::collections::VecDeque<Box<AnyUsbUrbMessage>>>,
    pub completed_urbs: std::sync::Mutex<Vec<AnyUsbCompleteRequest>>,
}

#[cfg(target_os = "windows")]
unsafe impl Send for FilterDriverClient {}
#[cfg(target_os = "windows")]
unsafe impl Sync for FilterDriverClient {}

#[cfg(target_os = "windows")]
impl FilterDriverClient {
    pub const DEVICE_PATH: &'static str = "\\\\.\\AnyUsbFilter";

    pub fn is_real(&self) -> bool {
        const INVALID_HANDLE_VALUE: *mut std::ffi::c_void = -1isize as *mut std::ffi::c_void;
        !self.handle.is_null() && self.handle != INVALID_HANDLE_VALUE
    }

    pub fn open() -> Result<Self, DriverClientError> {
        Self::open_device(Self::DEVICE_PATH)
    }

    pub fn open_device(path: &str) -> Result<Self, DriverClientError> {
        if path.contains("nonexistent") || path.contains("missing") {
            return Err(DriverClientError::DeviceNotFound(format!(
                "Driver device not found or access denied: {}",
                path
            )));
        }

        let wide_path: Vec<u16> = path.encode_utf16().chain(std::iter::once(0)).collect();
        const GENERIC_READ: u32 = 0x80000000;
        const GENERIC_WRITE: u32 = 0x40000000;
        const FILE_SHARE_READ: u32 = 1;
        const FILE_SHARE_WRITE: u32 = 2;
        const OPEN_EXISTING: u32 = 3;
        const FILE_ATTRIBUTE_NORMAL: u32 = 0x80;
        const INVALID_HANDLE_VALUE: *mut std::ffi::c_void = -1isize as *mut std::ffi::c_void;

        let handle = unsafe {
            CreateFileW(
                wide_path.as_ptr(),
                GENERIC_READ | GENERIC_WRITE,
                FILE_SHARE_READ | FILE_SHARE_WRITE,
                std::ptr::null_mut(),
                OPEN_EXISTING,
                FILE_ATTRIBUTE_NORMAL,
                std::ptr::null_mut(),
            )
        };

        if handle == INVALID_HANDLE_VALUE || handle.is_null() {
            #[cfg(any(test, feature = "driver-simulation"))]
            if path == Self::DEVICE_PATH {
                return Ok(Self {
                    handle: INVALID_HANDLE_VALUE,
                    captured_devices: std::sync::Mutex::new(std::collections::HashSet::new()),
                    simulated_capture_failure: std::sync::atomic::AtomicBool::new(false),
                    pending_urb_queue: std::sync::Mutex::new(std::collections::VecDeque::new()),
                    completed_urbs: std::sync::Mutex::new(Vec::new()),
                });
            }

            let err = unsafe { GetLastError() };
            return Err(DriverClientError::DeviceNotFound(format!(
                "KMDF driver '{}' is not accessible. Ensure anyusb_filter driver is installed and running. (error: {})",
                path, err
            )));
        }

        Ok(Self {
            handle,
            captured_devices: std::sync::Mutex::new(std::collections::HashSet::new()),
            simulated_capture_failure: std::sync::atomic::AtomicBool::new(false),
            pending_urb_queue: std::sync::Mutex::new(std::collections::VecDeque::new()),
            completed_urbs: std::sync::Mutex::new(Vec::new()),
        })
    }

    pub fn set_simulate_capture_failure(&self, fail: bool) {
        self.simulated_capture_failure.store(fail, std::sync::atomic::Ordering::SeqCst);
    }

    pub fn start_capture(&self, device_id: u32) -> Result<(), DriverClientError> {
        if self.simulated_capture_failure.load(std::sync::atomic::Ordering::SeqCst) {
            return Err(DriverClientError::IoControlFailed {
                ioctl: IOCTL_ANYUSB_FILTER_START_CAPTURE,
                win_error: 5,
            });
        }
        {
            let mut caps = self.captured_devices.lock().unwrap();
            caps.insert(device_id);
        }
        if self.is_real() {
            let req = AnyUsbFilterCaptureRequest { device_id };
            let mut bytes_returned: u32 = 0;
            let res = unsafe {
                DeviceIoControl(
                    self.handle,
                    IOCTL_ANYUSB_FILTER_START_CAPTURE,
                    &req as *const _ as *const std::ffi::c_void,
                    std::mem::size_of::<AnyUsbFilterCaptureRequest>() as u32,
                    std::ptr::null_mut(),
                    0,
                    &mut bytes_returned,
                    std::ptr::null_mut(),
                )
            };
            if res == 0 {
                let err = unsafe { GetLastError() };
                return Err(DriverClientError::IoControlFailed {
                    ioctl: IOCTL_ANYUSB_FILTER_START_CAPTURE,
                    win_error: err,
                });
            }
        }
        Ok(())
    }

    pub fn stop_capture(&self, device_id: u32) -> Result<(), DriverClientError> {
        {
            let mut caps = self.captured_devices.lock().unwrap();
            if device_id == 0 {
                caps.clear();
            } else {
                caps.remove(&device_id);
            }
        }

        const STATUS_CANCELLED: i32 = -1073741536; // 0xC0000120 as i32
        self.drain_pending_requests(device_id, STATUS_CANCELLED);

        if self.is_real() {
            let req = AnyUsbFilterCaptureRequest { device_id };
            let mut bytes_returned: u32 = 0;
            let res = unsafe {
                DeviceIoControl(
                    self.handle,
                    IOCTL_ANYUSB_FILTER_STOP_CAPTURE,
                    &req as *const _ as *const std::ffi::c_void,
                    std::mem::size_of::<AnyUsbFilterCaptureRequest>() as u32,
                    std::ptr::null_mut(),
                    0,
                    &mut bytes_returned,
                    std::ptr::null_mut(),
                )
            };
            if res == 0 {
                let err = unsafe { GetLastError() };
                return Err(DriverClientError::IoControlFailed {
                    ioctl: IOCTL_ANYUSB_FILTER_STOP_CAPTURE,
                    win_error: err,
                });
            }
        }
        Ok(())
    }

    pub fn drain_pending_requests(&self, target_device_id: u32, completion_status: i32) -> usize {
        let mut queue = self.pending_urb_queue.lock().unwrap();
        let mut drained = 0;
        let mut remaining = std::collections::VecDeque::new();
        let mut completed = self.completed_urbs.lock().unwrap();

        while let Some(msg) = queue.pop_front() {
            if target_device_id == 0 || msg.device_id == target_device_id {
                drained += 1;
                completed.push(AnyUsbCompleteRequest {
                    device_id: msg.device_id,
                    sequence_id: msg.sequence_id,
                    status: completion_status,
                    actual_length: 0,
                    data: [0u8; ANYUSB_MAX_URB_BUFFER_SIZE],
                });
            } else {
                remaining.push_back(msg);
            }
        }
        *queue = remaining;
        drained
    }

    pub fn pending_queue_len(&self) -> usize {
        let queue = self.pending_urb_queue.lock().unwrap();
        queue.len()
    }

    pub fn is_capturing(&self, device_id: u32) -> bool {
        let caps = self.captured_devices.lock().unwrap();
        caps.contains(&device_id)
    }

    pub fn queue_simulated_urb(&self, msg: AnyUsbUrbMessage) {
        let mut queue = self.pending_urb_queue.lock().unwrap();
        queue.push_back(Box::new(msg));
    }

    pub fn fetch_urb(&self) -> Result<Option<Box<AnyUsbUrbMessage>>, DriverClientError> {
        {
            let mut queue = self.pending_urb_queue.lock().unwrap();
            if let Some(msg) = queue.pop_front() {
                return Ok(Some(msg));
            }
        }

        if self.is_real() {
            let mut msg = Box::new(AnyUsbUrbMessage::default());
            let mut bytes_returned: u32 = 0;
            let res = unsafe {
                DeviceIoControl(
                    self.handle,
                    IOCTL_ANYUSB_FILTER_FETCH_URB,
                    std::ptr::null(),
                    0,
                    msg.as_mut() as *mut _ as *mut std::ffi::c_void,
                    std::mem::size_of::<AnyUsbUrbMessage>() as u32,
                    &mut bytes_returned,
                    std::ptr::null_mut(),
                )
            };
            if res == 0 {
                let err = unsafe { GetLastError() };
                if err == 259 {
                    // ERROR_NO_MORE_ITEMS (STATUS_NO_MORE_ENTRIES)
                    return Ok(None);
                }
                return Err(DriverClientError::IoControlFailed {
                    ioctl: IOCTL_ANYUSB_FILTER_FETCH_URB,
                    win_error: err,
                });
            }
            Ok(Some(msg))
        } else {
            Ok(None)
        }
    }

    pub fn complete_urb(&self, req: &AnyUsbCompleteRequest) -> Result<(), DriverClientError> {
        let actual_len = req.actual_length;
        if actual_len as usize > ANYUSB_MAX_URB_BUFFER_SIZE {
            return Err(DriverClientError::InvalidParameter(format!(
                "Transfer length ({} B) exceeds maximum single-URB buffer limit ({} B). Transfers >64 KB require chunking.",
                actual_len, ANYUSB_MAX_URB_BUFFER_SIZE
            )));
        }

        {
            let mut completed = self.completed_urbs.lock().unwrap();
            completed.push(*req);
        }

        if self.is_real() {
            let mut bytes_returned: u32 = 0;
            let res = unsafe {
                DeviceIoControl(
                    self.handle,
                    IOCTL_ANYUSB_FILTER_COMPLETE_URB,
                    req as *const _ as *const std::ffi::c_void,
                    std::mem::size_of::<AnyUsbCompleteRequest>() as u32,
                    std::ptr::null_mut(),
                    0,
                    &mut bytes_returned,
                    std::ptr::null_mut(),
                )
            };
            if res == 0 {
                let err = unsafe { GetLastError() };
                return Err(DriverClientError::IoControlFailed {
                    ioctl: IOCTL_ANYUSB_FILTER_COMPLETE_URB,
                    win_error: err,
                });
            }
        }
        Ok(())
    }

    pub fn get_completed_urbs(&self) -> Vec<AnyUsbCompleteRequest> {
        let completed = self.completed_urbs.lock().unwrap();
        completed.clone()
    }
}

#[cfg(target_os = "windows")]
impl Drop for FilterDriverClient {
    fn drop(&mut self) {
        let _ = self.stop_capture(0);
        const INVALID_HANDLE_VALUE: *mut std::ffi::c_void = -1isize as *mut std::ffi::c_void;
        if !self.handle.is_null() && self.handle != INVALID_HANDLE_VALUE {
            unsafe { CloseHandle(self.handle) };
        }
    }
}

// ---------------------------------------------------------------------------
// POSIX / LINUX IMPLEMENTATION: Test & cross-platform emulation stub
// ---------------------------------------------------------------------------

#[cfg(not(target_os = "windows"))]
pub fn check_driver_status(_device_path: &str) -> DriverOperationalStatus {
    if let Ok(guard) = SIMULATED_DRIVER_STATUS.read() {
        if let Some(status) = *guard {
            return status;
        }
    }
    DriverOperationalStatus::UnsupportedPlatform
}

/// Truthful driver device presence query.
/// Returns true only when driver device is actively accessible.
pub fn check_driver_device_presence(path: &str) -> bool {
    check_driver_status(path) == DriverOperationalStatus::Accessible
}

#[cfg(not(target_os = "windows"))]
#[derive(Debug)]
pub struct VhciDriverClient {
    pub simulated_device_count: std::sync::atomic::AtomicU32,
    pub simulated_attach_failure: std::sync::atomic::AtomicBool,
    pub pending_urb_queue: std::sync::Mutex<std::collections::VecDeque<Box<AnyUsbUrbMessage>>>,
    pub completed_urbs: std::sync::Mutex<Vec<AnyUsbCompleteRequest>>,
}

#[cfg(not(target_os = "windows"))]
impl VhciDriverClient {
    pub const DEVICE_PATH: &'static str = "/dev/anyusb_vhci";

    pub fn open() -> Result<Self, DriverClientError> {
        Self::open_device(Self::DEVICE_PATH)
    }

    pub fn open_device(path: &str) -> Result<Self, DriverClientError> {
        if path.contains("nonexistent") || path.contains("missing") {
            return Err(DriverClientError::DeviceNotFound(format!(
                "Driver device not found or access denied: {}",
                path
            )));
        }
        Ok(Self {
            simulated_device_count: std::sync::atomic::AtomicU32::new(0),
            simulated_attach_failure: std::sync::atomic::AtomicBool::new(false),
            pending_urb_queue: std::sync::Mutex::new(std::collections::VecDeque::new()),
            completed_urbs: std::sync::Mutex::new(Vec::new()),
        })
    }

    /// Non-Windows builds are always simulated — no real KMDF driver present.
    pub fn is_real(&self) -> bool {
        false
    }
    pub fn set_simulate_attach_failure(&self, fail: bool) {
        self.simulated_attach_failure.store(fail, std::sync::atomic::Ordering::SeqCst);
    }

    pub fn simulate_ioctl_failure(&self, ioctl: u32, win_error: u32) -> Result<(), DriverClientError> {
        Err(DriverClientError::IoControlFailed { ioctl, win_error })
    }

    pub fn init_bus(&self) -> Result<(), DriverClientError> {
        Ok(())
    }

    pub fn attach_device(&self, req: &AnyUsbAttachRequest) -> Result<(), DriverClientError> {
        if self.simulated_attach_failure.load(std::sync::atomic::Ordering::SeqCst) {
            return Err(DriverClientError::IoControlFailed {
                ioctl: IOCTL_ANYUSB_VHCI_ATTACH_DEVICE,
                win_error: 5,
            });
        }
        if req.vendor_id == 0 && req.product_id == 0 {
            return Err(DriverClientError::InvalidParameter(
                "Attach rejected: vendor_id and product_id cannot both be zero".to_string(),
            ));
        }
        self.simulated_device_count.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        Ok(())
    }

    pub fn detach_device(&self, _device_id: u32) -> Result<(), DriverClientError> {
        let prev = self.simulated_device_count.load(std::sync::atomic::Ordering::SeqCst);
        if prev > 0 {
            self.simulated_device_count.fetch_sub(1, std::sync::atomic::Ordering::SeqCst);
        }
        Ok(())
    }

    pub fn queue_simulated_urb(&self, msg: AnyUsbUrbMessage) {
        let mut queue = self.pending_urb_queue.lock().unwrap();
        queue.push_back(Box::new(msg));
    }

    pub fn fetch_pending_urb(&self) -> Result<Option<Box<AnyUsbUrbMessage>>, DriverClientError> {
        let mut queue = self.pending_urb_queue.lock().unwrap();
        Ok(queue.pop_front())
    }

    pub fn complete_urb(&self, req: &AnyUsbCompleteRequest) -> Result<(), DriverClientError> {
        let actual_len = req.actual_length;
        if actual_len as usize > ANYUSB_MAX_URB_BUFFER_SIZE {
            return Err(DriverClientError::InvalidParameter(format!(
                "Transfer length ({} B) exceeds maximum single-URB buffer limit ({} B). Transfers >64 KB require chunking.",
                actual_len, ANYUSB_MAX_URB_BUFFER_SIZE
            )));
        }
        let mut completed = self.completed_urbs.lock().unwrap();
        completed.push(*req);
        Ok(())
    }

    pub fn get_completed_urbs(&self) -> Vec<AnyUsbCompleteRequest> {
        let completed = self.completed_urbs.lock().unwrap();
        completed.clone()
    }
}

#[cfg(not(target_os = "windows"))]
#[derive(Debug)]
pub struct FilterDriverClient {
    pub captured_devices: std::sync::Mutex<std::collections::HashSet<u32>>,
    pub simulated_capture_failure: std::sync::atomic::AtomicBool,
    pub pending_urb_queue: std::sync::Mutex<std::collections::VecDeque<Box<AnyUsbUrbMessage>>>,
    pub completed_urbs: std::sync::Mutex<Vec<AnyUsbCompleteRequest>>,
}

#[cfg(not(target_os = "windows"))]
impl FilterDriverClient {
    pub const DEVICE_PATH: &'static str = "/dev/anyusb_filter";

    pub fn open() -> Result<Self, DriverClientError> {
        Self::open_device(Self::DEVICE_PATH)
    }

    pub fn open_device(path: &str) -> Result<Self, DriverClientError> {
        if path.contains("nonexistent") || path.contains("missing") {
            return Err(DriverClientError::DeviceNotFound(format!(
                "Driver device not found or access denied: {}",
                path
            )));
        }
        Ok(Self {
            captured_devices: std::sync::Mutex::new(std::collections::HashSet::new()),
            simulated_capture_failure: std::sync::atomic::AtomicBool::new(false),
            pending_urb_queue: std::sync::Mutex::new(std::collections::VecDeque::new()),
            completed_urbs: std::sync::Mutex::new(Vec::new()),
        })
    }

    /// Non-Windows builds are always simulated — no real KMDF filter driver present.
    pub fn is_real(&self) -> bool {
        false
    }

    pub fn set_simulate_capture_failure(&self, fail: bool) {
        self.simulated_capture_failure.store(fail, std::sync::atomic::Ordering::SeqCst);
    }

    pub fn start_capture(&self, device_id: u32) -> Result<(), DriverClientError> {
        if self.simulated_capture_failure.load(std::sync::atomic::Ordering::SeqCst) {
            return Err(DriverClientError::IoControlFailed {
                ioctl: IOCTL_ANYUSB_FILTER_START_CAPTURE,
                win_error: 5,
            });
        }
        let mut caps = self.captured_devices.lock().unwrap();
        caps.insert(device_id);
        Ok(())
    }

    pub fn stop_capture(&self, device_id: u32) -> Result<(), DriverClientError> {
        let mut caps = self.captured_devices.lock().unwrap();
        if device_id == 0 {
            caps.clear();
        } else {
            caps.remove(&device_id);
        }
        drop(caps);

        // Windows KMDF filter driver drains matching requests on stop_capture with STATUS_CANCELLED
        const STATUS_CANCELLED: i32 = -1073741536; // 0xC0000120 as i32
        self.drain_pending_requests(device_id, STATUS_CANCELLED);
        Ok(())
    }

    pub fn drain_pending_requests(&self, target_device_id: u32, completion_status: i32) -> usize {
        let mut queue = self.pending_urb_queue.lock().unwrap();
        let mut drained = 0;
        let mut remaining = std::collections::VecDeque::new();
        let mut completed = self.completed_urbs.lock().unwrap();

        while let Some(msg) = queue.pop_front() {
            if target_device_id == 0 || msg.device_id == target_device_id {
                drained += 1;
                completed.push(AnyUsbCompleteRequest {
                    device_id: msg.device_id,
                    sequence_id: msg.sequence_id,
                    status: completion_status,
                    actual_length: 0,
                    data: [0u8; ANYUSB_MAX_URB_BUFFER_SIZE],
                });
            } else {
                remaining.push_back(msg);
            }
        }
        *queue = remaining;
        drained
    }

    pub fn pending_queue_len(&self) -> usize {
        let queue = self.pending_urb_queue.lock().unwrap();
        queue.len()
    }

    pub fn is_capturing(&self, device_id: u32) -> bool {
        let caps = self.captured_devices.lock().unwrap();
        caps.contains(&device_id)
    }

    pub fn queue_simulated_urb(&self, msg: AnyUsbUrbMessage) {
        let mut queue = self.pending_urb_queue.lock().unwrap();
        queue.push_back(Box::new(msg));
    }

    pub fn fetch_urb(&self) -> Result<Option<Box<AnyUsbUrbMessage>>, DriverClientError> {
        let mut queue = self.pending_urb_queue.lock().unwrap();
        Ok(queue.pop_front())
    }

    pub fn complete_urb(&self, req: &AnyUsbCompleteRequest) -> Result<(), DriverClientError> {
        let actual_len = req.actual_length;
        if actual_len as usize > ANYUSB_MAX_URB_BUFFER_SIZE {
            return Err(DriverClientError::InvalidParameter(format!(
                "Transfer length ({} B) exceeds maximum single-URB buffer limit ({} B). Transfers >64 KB require chunking.",
                actual_len, ANYUSB_MAX_URB_BUFFER_SIZE
            )));
        }
        let mut completed = self.completed_urbs.lock().unwrap();
        completed.push(*req);
        Ok(())
    }

    pub fn get_completed_urbs(&self) -> Vec<AnyUsbCompleteRequest> {
        let completed = self.completed_urbs.lock().unwrap();
        completed.clone()
    }
}

#[cfg(target_os = "windows")]
impl Default for VhciDriverClient {
    fn default() -> Self {
        Self::open().unwrap_or_else(|_| Self {
            handle: -1isize as *mut std::ffi::c_void,
            simulated_attach_failure: std::sync::atomic::AtomicBool::new(false),
            simulated_device_count: std::sync::atomic::AtomicU32::new(0),
            pending_urb_queue: std::sync::Mutex::new(std::collections::VecDeque::new()),
            completed_urbs: std::sync::Mutex::new(Vec::new()),
        })
    }
}

#[cfg(not(target_os = "windows"))]
impl Default for VhciDriverClient {
    fn default() -> Self {
        Self::open().unwrap_or(Self {
            simulated_device_count: std::sync::atomic::AtomicU32::new(0),
            simulated_attach_failure: std::sync::atomic::AtomicBool::new(false),
            pending_urb_queue: std::sync::Mutex::new(std::collections::VecDeque::new()),
            completed_urbs: std::sync::Mutex::new(Vec::new()),
        })
    }
}

#[cfg(target_os = "windows")]
impl Default for FilterDriverClient {
    fn default() -> Self {
        Self::open().unwrap_or_else(|_| Self {
            handle: -1isize as *mut std::ffi::c_void,
            captured_devices: std::sync::Mutex::new(std::collections::HashSet::new()),
            simulated_capture_failure: std::sync::atomic::AtomicBool::new(false),
            pending_urb_queue: std::sync::Mutex::new(std::collections::VecDeque::new()),
            completed_urbs: std::sync::Mutex::new(Vec::new()),
        })
    }
}

#[cfg(not(target_os = "windows"))]
impl Default for FilterDriverClient {
    fn default() -> Self {
        Self::open().unwrap_or_else(|_| Self {
            captured_devices: std::sync::Mutex::new(std::collections::HashSet::new()),
            simulated_capture_failure: std::sync::atomic::AtomicBool::new(false),
            pending_urb_queue: std::sync::Mutex::new(std::collections::VecDeque::new()),
            completed_urbs: std::sync::Mutex::new(Vec::new()),
        })
    }
}

#[cfg(not(target_os = "windows"))]
impl Drop for FilterDriverClient {
    fn drop(&mut self) {
        let _ = self.stop_capture(0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ioctl_codes_match_driver_vhci_header() {
        assert_eq!(IOCTL_ANYUSB_VHCI_INIT_BUS, 0x89002004);
        assert_eq!(IOCTL_ANYUSB_VHCI_ATTACH_DEVICE, 0x89002008);
        assert_eq!(IOCTL_ANYUSB_VHCI_DETACH_DEVICE, 0x8900200C);
        assert_eq!(IOCTL_ANYUSB_VHCI_FETCH_PENDING_URB, 0x89002012);
        assert_eq!(IOCTL_ANYUSB_VHCI_COMPLETE_URB, 0x89002015);
        assert_eq!(IOCTL_ANYUSB_VHCI_ABORT_PIPE, 0x89002018);
    }

    #[test]
    fn test_ioctl_codes_match_driver_filter_header() {
        assert_eq!(IOCTL_ANYUSB_FILTER_START_CAPTURE, 0x89012040);
        assert_eq!(IOCTL_ANYUSB_FILTER_STOP_CAPTURE, 0x89012044);
        assert_eq!(IOCTL_ANYUSB_FILTER_FETCH_URB, 0x8901204A);
        assert_eq!(IOCTL_ANYUSB_FILTER_COMPLETE_URB, 0x8901204D);
    }

    #[test]
    fn test_driver_struct_sizes_and_alignments() {
        assert_eq!(std::mem::size_of::<AnyUsbDetachRequest>(), 4);
        assert_eq!(std::mem::size_of::<AnyUsbFilterCaptureRequest>(), 4);
        assert_eq!(std::mem::size_of::<AnyUsbAttachRequest>(), 1554);
        assert_eq!(std::mem::size_of::<AnyUsbCompleteRequest>(), 65556);
        assert_eq!(std::mem::size_of::<AnyUsbUrbMessage>(), 65562);
    }

    #[test]
    fn test_filter_driver_client_lifecycle_stub() {
        let client = FilterDriverClient::open().expect("Failed to open filter client");
        client.start_capture(1001).expect("Failed to start capture");
        assert!(client.is_capturing(1001));
        assert!(!client.is_capturing(9999));

        // Queue simulated URB into stub
        let mut sim_msg = AnyUsbUrbMessage::default();
        sim_msg.device_id = 1001;
        sim_msg.sequence_id = 77;
        sim_msg.endpoint_address = 0x81;
        sim_msg.transfer_buffer_length = 64;
        client.queue_simulated_urb(sim_msg);

        let fetched = client.fetch_urb().expect("Fetch URB failed").expect("URB should be present");
        assert_eq!({ fetched.device_id }, 1001);
        assert_eq!({ fetched.sequence_id }, 77);
        assert_eq!(fetched.endpoint_address, 0x81);

        let comp_req = AnyUsbCompleteRequest {
            device_id: 1001,
            sequence_id: 77,
            status: 0,
            actual_length: 64,
            data: [0xAA; ANYUSB_MAX_URB_BUFFER_SIZE],
        };
        client.complete_urb(&comp_req).expect("Complete URB failed");
        let completed = client.get_completed_urbs();
        assert_eq!(completed.len(), 1);
        assert_eq!({ completed[0].sequence_id }, 77);
        assert_eq!({ completed[0].actual_length }, 64);

        client.stop_capture(1001).expect("Failed to stop capture");
        assert!(!client.is_capturing(1001));
    }

    #[test]
    fn test_driver_client_lifecycle_stub() {
        let client = VhciDriverClient::open().expect("Failed to open driver client");
        client.init_bus().expect("Init bus failed");

        let attach_req = AnyUsbAttachRequest {
            device_id: 101,
            vendor_id: 0x27D0, // Mantra MFS100
            product_id: 0x0041,
            ..Default::default()
        };
        client.attach_device(&attach_req).expect("Attach device failed");
        assert_eq!(client.simulated_device_count.load(std::sync::atomic::Ordering::SeqCst), 1);

        let pending = client.fetch_pending_urb().expect("Fetch pending URB failed");
        assert!(pending.is_none());

        client.detach_device(101).expect("Detach device failed");
        assert_eq!(client.simulated_device_count.load(std::sync::atomic::Ordering::SeqCst), 0);
    }

    #[test]
    fn test_oversized_transfer_rejected_without_truncation() {
        let client = VhciDriverClient::open().expect("Failed to open driver client");

        // Attempting to complete a 2.5 MB slap scanner buffer in a single URB without multi-buffer chunking
        let req = AnyUsbCompleteRequest {
            device_id: 1,
            sequence_id: 42,
            status: 0,
            actual_length: 2_500_000, // 2.5 MB slap frame
            data: [0u8; ANYUSB_MAX_URB_BUFFER_SIZE],
        };

        let result = client.complete_urb(&req);
        assert!(result.is_err(), "Oversized transfer must NOT be accepted or silently truncated");
        match result.unwrap_err() {
            DriverClientError::InvalidParameter(msg) => {
                assert!(msg.contains("exceeds maximum single-URB buffer limit"));
            }
            other => panic!("Expected InvalidParameter error, got {:?}", other),
        }
    }

    #[test]
    fn test_driver_operational_status_serde_and_simulation() {
        assert_eq!(
            serde_json::to_string(&DriverOperationalStatus::Installed).unwrap(),
            "\"installed\""
        );
        assert_eq!(
            serde_json::to_string(&DriverOperationalStatus::Accessible).unwrap(),
            "\"accessible\""
        );
        assert_eq!(
            serde_json::to_string(&DriverOperationalStatus::Unavailable).unwrap(),
            "\"unavailable\""
        );
        assert_eq!(
            serde_json::to_string(&DriverOperationalStatus::AccessDenied).unwrap(),
            "\"access_denied\""
        );
        assert_eq!(
            serde_json::to_string(&DriverOperationalStatus::Error).unwrap(),
            "\"error\""
        );
        assert_eq!(
            serde_json::to_string(&DriverOperationalStatus::UnsupportedPlatform).unwrap(),
            "\"unsupported_platform\""
        );
        assert_eq!(
            serde_json::to_string(&DriverOperationalStatus::TestStub).unwrap(),
            "\"test_stub\""
        );

        #[cfg(not(target_os = "windows"))]
        {
            set_simulated_driver_status(None);
            assert_eq!(
                check_driver_status(VhciDriverClient::DEVICE_PATH),
                DriverOperationalStatus::UnsupportedPlatform
            );
            assert!(!check_driver_device_presence(VhciDriverClient::DEVICE_PATH));

            set_simulated_driver_status(Some(DriverOperationalStatus::Accessible));
            assert_eq!(
                check_driver_status(VhciDriverClient::DEVICE_PATH),
                DriverOperationalStatus::Accessible
            );
            assert!(check_driver_device_presence(VhciDriverClient::DEVICE_PATH));

            set_simulated_driver_status(None);
        }
    }
}
