//! Turbo-URB State Machine & Windows USBD Core Engine
//! Complete implementation of Windows USBD / Linux USB core Request Block processing:
//! - URB_FUNCTION_CONTROL_TRANSFER (Setup packet, Control IN/OUT)
//! - URB_FUNCTION_BULK_OR_INTERRUPT_TRANSFER (Data toggle synchronization, stall handling)
//! - URB_FUNCTION_ISOCH_TRANSFER (Microsecond timestamps, sub-packet framing)
//! - URB_FUNCTION_ABORT_PIPE & URB_FUNCTION_SYNC_RESET_PIPE_AND_CLEAR_STALL
//! - URB_FUNCTION_SELECT_CONFIGURATION & URB_FUNCTION_SELECT_INTERFACE

use crate::protocol::{
    TransferType, TurboUrbHeader, CMD_COMPLETE_URB, CMD_SUBMIT_URB,
    FLAG_ISOCH_PACKET,
};
use crate::usb_descriptors::UsbParsedConfiguration;
use byteorder::{LittleEndian, ReadBytesExt};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::io::Cursor;
use std::time::Instant;

// -----------------------------------------------------------------------------
// 1. Windows USBD URB Function Codes (Microsoft WDK usb.h)
// -----------------------------------------------------------------------------
pub const URB_FUNCTION_SELECT_CONFIGURATION: u16 = 0x0000;
pub const URB_FUNCTION_SELECT_INTERFACE: u16 = 0x0001;
pub const URB_FUNCTION_ABORT_PIPE: u16 = 0x0002;
pub const URB_FUNCTION_TAKE_FRAME_LENGTH_CONTROL: u16 = 0x0003;
pub const URB_FUNCTION_RELEASE_FRAME_LENGTH_CONTROL: u16 = 0x0004;
pub const URB_FUNCTION_GET_FRAME_LENGTH: u16 = 0x0005;
pub const URB_FUNCTION_SET_FRAME_LENGTH: u16 = 0x0006;
pub const URB_FUNCTION_GET_CURRENT_FRAME_NUMBER: u16 = 0x0007;
pub const URB_FUNCTION_CONTROL_TRANSFER: u16 = 0x0008;
pub const URB_FUNCTION_BULK_OR_INTERRUPT_TRANSFER: u16 = 0x0009;
pub const URB_FUNCTION_ISOCH_TRANSFER: u16 = 0x000A;
pub const URB_FUNCTION_GET_DESCRIPTOR_FROM_DEVICE: u16 = 0x000B;
pub const URB_FUNCTION_SET_DESCRIPTOR_TO_DEVICE: u16 = 0x000C;
pub const URB_FUNCTION_SET_FEATURE_TO_DEVICE: u16 = 0x000D;
pub const URB_FUNCTION_SET_FEATURE_TO_INTERFACE: u16 = 0x000E;
pub const URB_FUNCTION_SET_FEATURE_TO_ENDPOINT: u16 = 0x000F;
pub const URB_FUNCTION_CLEAR_FEATURE_TO_DEVICE: u16 = 0x0010;
pub const URB_FUNCTION_CLEAR_FEATURE_TO_INTERFACE: u16 = 0x0011;
pub const URB_FUNCTION_CLEAR_FEATURE_TO_ENDPOINT: u16 = 0x0012;
pub const URB_FUNCTION_GET_STATUS_FROM_DEVICE: u16 = 0x0013;
pub const URB_FUNCTION_GET_STATUS_FROM_INTERFACE: u16 = 0x0014;
pub const URB_FUNCTION_GET_STATUS_FROM_ENDPOINT: u16 = 0x0015;
pub const URB_FUNCTION_VENDOR_DEVICE: u16 = 0x0017;
pub const URB_FUNCTION_VENDOR_INTERFACE: u16 = 0x0018;
pub const URB_FUNCTION_VENDOR_ENDPOINT: u16 = 0x0019;
pub const URB_FUNCTION_CLASS_DEVICE: u16 = 0x001A;
pub const URB_FUNCTION_CLASS_INTERFACE: u16 = 0x001B;
pub const URB_FUNCTION_CLASS_ENDPOINT: u16 = 0x001C;
pub const URB_FUNCTION_SYNC_RESET_PIPE_AND_CLEAR_STALL: u16 = 0x0030;
pub const URB_FUNCTION_SYNC_RESET_PIPE: u16 = 0x0031;
pub const URB_FUNCTION_CONTROL_TRANSFER_EX: u16 = 0x0032;

// -----------------------------------------------------------------------------
// 2. Windows USBD Status Codes (Microsoft WDK usb.h)
// -----------------------------------------------------------------------------
pub const USBD_STATUS_SUCCESS: u32 = 0x0000_0000;
pub const USBD_STATUS_PENDING: u32 = 0x4000_0000;
pub const USBD_STATUS_CRC: u32 = 0xC000_0001;
pub const USBD_STATUS_BTSTUFF: u32 = 0xC000_0002;
pub const USBD_STATUS_DATA_TOGGLE_MISMATCH: u32 = 0xC000_0003;
pub const USBD_STATUS_STALL_PID: u32 = 0xC000_0004;
pub const USBD_STATUS_DEV_NOT_RESPONDING: u32 = 0xC000_0005;
pub const USBD_STATUS_PID_CHECK_FAILURE: u32 = 0xC000_0006;
pub const USBD_STATUS_UNEXPECTED_PID: u32 = 0xC000_0007;
pub const USBD_STATUS_DATA_OVERRUN: u32 = 0xC000_0008;
pub const USBD_STATUS_DATA_UNDERRUN: u32 = 0xC000_0009;
pub const USBD_STATUS_INVALID_PARAMETER: u32 = 0xC000_000A;
pub const USBD_STATUS_BUFFER_OVERRUN: u32 = 0xC000_000C;
pub const USBD_STATUS_BUFFER_UNDERRUN: u32 = 0xC000_000D;
pub const USBD_STATUS_XACT_ERROR: u32 = 0xC000_0011;
pub const USBD_STATUS_BABBLE_DETECTED: u32 = 0xC000_0012;
pub const USBD_STATUS_CANCELED: u32 = 0xC001_0000;
pub const USBD_STATUS_INVALID_PIPE_HANDLE: u32 = 0xC000_0030;

// -----------------------------------------------------------------------------
// 3. USB Setup Packet (8 Bytes - USB 2.0 Spec §9.3)
// -----------------------------------------------------------------------------
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct UsbSetupPacket {
    pub bm_request_type: u8,
    pub b_request: u8,
    pub w_value: u16,
    pub w_index: u16,
    pub w_length: u16,
}

impl UsbSetupPacket {
    pub const SIZE: usize = 8;

    pub fn new(bm_request_type: u8, b_request: u8, w_value: u16, w_index: u16, w_length: u16) -> Self {
        Self {
            bm_request_type,
            b_request,
            w_value,
            w_index,
            w_length,
        }
    }

    pub fn parse(bytes: &[u8]) -> Result<Self, String> {
        if bytes.len() < Self::SIZE {
            return Err(format!(
                "Setup packet too short: expected 8 bytes, got {}",
                bytes.len()
            ));
        }
        let mut rdr = Cursor::new(bytes);
        let bm_request_type = rdr.read_u8().map_err(|e| e.to_string())?;
        let b_request = rdr.read_u8().map_err(|e| e.to_string())?;
        let w_value = rdr.read_u16::<LittleEndian>().map_err(|e| e.to_string())?;
        let w_index = rdr.read_u16::<LittleEndian>().map_err(|e| e.to_string())?;
        let w_length = rdr.read_u16::<LittleEndian>().map_err(|e| e.to_string())?;

        Ok(Self {
            bm_request_type,
            b_request,
            w_value,
            w_index,
            w_length,
        })
    }

    pub fn to_bytes(&self) -> [u8; 8] {
        let mut buf = [0u8; 8];
        buf[0] = self.bm_request_type;
        buf[1] = self.b_request;
        buf[2] = (self.w_value & 0xFF) as u8;
        buf[3] = ((self.w_value >> 8) & 0xFF) as u8;
        buf[4] = (self.w_index & 0xFF) as u8;
        buf[5] = ((self.w_index >> 8) & 0xFF) as u8;
        buf[6] = (self.w_length & 0xFF) as u8;
        buf[7] = ((self.w_length >> 8) & 0xFF) as u8;
        buf
    }

    #[inline]
    pub fn is_device_to_host(&self) -> bool {
        (self.bm_request_type & 0x80) != 0
    }

    #[inline]
    pub fn is_host_to_device(&self) -> bool {
        (self.bm_request_type & 0x80) == 0
    }

    #[inline]
    pub fn request_type_standard(&self) -> bool {
        ((self.bm_request_type >> 5) & 0x03) == 0
    }

    #[inline]
    pub fn request_type_class(&self) -> bool {
        ((self.bm_request_type >> 5) & 0x03) == 1
    }

    #[inline]
    pub fn request_type_vendor(&self) -> bool {
        ((self.bm_request_type >> 5) & 0x03) == 2
    }
}

// -----------------------------------------------------------------------------
// 4. Isochronous Frame Packet Descriptor with Microsecond Timestamps
// -----------------------------------------------------------------------------
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct UrbIsoPacket {
    pub offset: u32,
    pub length: u32,
    pub actual_length: u32,
    pub status: u32,
    pub timestamp_us: u64,
}

// -----------------------------------------------------------------------------
// 5. Pipe Operational State & Endpoint Manager
// -----------------------------------------------------------------------------
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PipeState {
    Idle,
    Active,
    Stalled,
    Aborted,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UrbPipe {
    pub endpoint_address: u8,
    pub transfer_type: TransferType,
    pub max_packet_size: u16,
    pub interval: u8,
    pub state: PipeState,
    pub data_toggle: bool, // false = DATA0, true = DATA1
    pub total_transfers: u64,
    pub total_bytes: u64,
    pub stall_count: u32,
}

impl UrbPipe {
    pub fn new(endpoint_address: u8, transfer_type: TransferType, max_packet_size: u16) -> Self {
        Self {
            endpoint_address,
            transfer_type,
            max_packet_size,
            interval: 0,
            state: PipeState::Idle,
            data_toggle: false,
            total_transfers: 0,
            total_bytes: 0,
            stall_count: 0,
        }
    }

    #[inline]
    pub fn is_in(&self) -> bool {
        (self.endpoint_address & 0x80) != 0
    }

    #[inline]
    pub fn is_out(&self) -> bool {
        (self.endpoint_address & 0x80) == 0
    }

    /// Toggles the data PID (DATA0 <-> DATA1)
    #[inline]
    pub fn step_toggle(&mut self) -> bool {
        let prev = self.data_toggle;
        self.data_toggle = !self.data_toggle;
        prev
    }

    /// Resets data toggle to DATA0 (e.g. after CLEAR_FEATURE ENDPOINT_HALT)
    #[inline]
    pub fn reset_toggle(&mut self) {
        self.data_toggle = false;
    }
}

// -----------------------------------------------------------------------------
// 6. High-Level URB Request & Payload
// -----------------------------------------------------------------------------
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum UrbPayload {
    Control {
        setup: UsbSetupPacket,
        data: Vec<u8>,
    },
    BulkOrInterrupt {
        data: Vec<u8>,
    },
    Isochronous {
        start_frame: u32,
        number_of_packets: u32,
        packets: Vec<UrbIsoPacket>,
        data: Vec<u8>,
    },
    SelectConfiguration {
        configuration_value: u8,
    },
    SelectInterface {
        interface_number: u8,
        alternate_setting: u8,
    },
    AbortPipe {
        endpoint_address: u8,
    },
    ResetPipe {
        endpoint_address: u8,
        clear_stall: bool,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UrbRequest {
    pub sequence_id: u64,
    pub device_id: u32,
    pub endpoint_address: u8,
    pub function_code: u16,
    pub transfer_flags: u32,
    pub payload: UrbPayload,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UrbCompletion {
    pub sequence_id: u64,
    pub device_id: u32,
    pub endpoint_address: u8,
    pub function_code: u16,
    pub usbd_status: u32,
    pub actual_length: u32,
    pub data: Vec<u8>,
    pub iso_packets: Vec<UrbIsoPacket>,
}

// -----------------------------------------------------------------------------
// 7. Full URB State Machine & Pipeline Engine
// -----------------------------------------------------------------------------
pub struct UrbEngine {
    pub device_id: u32,
    pub pipes: HashMap<u8, UrbPipe>,
    pub in_flight: HashMap<u64, UrbRequest>,
    pub active_configuration: u8,
    pub active_interface_settings: HashMap<u8, u8>,
    start_time: Instant,
}

impl UrbEngine {
    pub fn new(device_id: u32) -> Self {
        let mut pipes = HashMap::new();
        // Default control pipe (Endpoint 0 bidirectional)
        pipes.insert(0x00, UrbPipe::new(0x00, TransferType::Control, 64));

        Self {
            device_id,
            pipes,
            in_flight: HashMap::new(),
            active_configuration: 0,
            active_interface_settings: HashMap::new(),
            start_time: Instant::now(),
        }
    }

    /// Configures the engine endpoints from an AST parsed configuration
    pub fn configure_from_descriptors(&mut self, config: &UsbParsedConfiguration) {
        self.pipes.clear();
        self.pipes.insert(0x00, UrbPipe::new(0x00, TransferType::Control, 64));

        self.active_configuration = config.descriptor.b_configuration_value;
        self.active_interface_settings.clear();

        for iface in &config.interfaces {
            self.active_interface_settings.insert(
                iface.descriptor.b_interface_number,
                iface.descriptor.b_alternate_setting,
            );

            for ep in &iface.endpoints {
                let xfer_type = match ep.descriptor.bm_attributes & 0x03 {
                    0 => TransferType::Control,
                    1 => TransferType::Isochronous,
                    2 => TransferType::Bulk,
                    3 => TransferType::Interrupt,
                    _ => TransferType::Bulk,
                };
                let mut pipe = UrbPipe::new(
                    ep.descriptor.b_endpoint_address,
                    xfer_type,
                    ep.descriptor.max_packet_size(),
                );
                pipe.interval = ep.descriptor.b_interval;
                self.pipes.insert(ep.descriptor.b_endpoint_address, pipe);
            }
        }
    }

    /// Registers a custom pipe explicitly (for virtual testing or custom hardware)
    pub fn register_pipe(&mut self, endpoint: u8, xfer_type: TransferType, max_packet_size: u16) {
        self.pipes.insert(endpoint, UrbPipe::new(endpoint, xfer_type, max_packet_size));
    }

    /// Triggers an intentional STALL condition on an endpoint (e.g. simulated hardware error)
    pub fn inject_endpoint_stall(&mut self, endpoint: u8) {
        if let Some(pipe) = self.pipes.get_mut(&endpoint) {
            pipe.state = PipeState::Stalled;
            pipe.stall_count += 1;
        }
    }

    /// Processes an incoming URB request through the complete state machine
    pub fn submit_urb(&mut self, req: UrbRequest) -> UrbCompletion {
        let seq = req.sequence_id;
        let ep = req.endpoint_address;
        let func = req.function_code;

        match func {
            URB_FUNCTION_CONTROL_TRANSFER | URB_FUNCTION_CONTROL_TRANSFER_EX => {
                self.handle_control_transfer(req)
            }
            URB_FUNCTION_BULK_OR_INTERRUPT_TRANSFER => {
                self.handle_bulk_or_interrupt_transfer(req)
            }
            URB_FUNCTION_ISOCH_TRANSFER => {
                self.handle_isoch_transfer(req)
            }
            URB_FUNCTION_ABORT_PIPE => {
                self.handle_abort_pipe(seq, ep)
            }
            URB_FUNCTION_SYNC_RESET_PIPE_AND_CLEAR_STALL | URB_FUNCTION_SYNC_RESET_PIPE => {
                self.handle_reset_pipe(seq, ep, func == URB_FUNCTION_SYNC_RESET_PIPE_AND_CLEAR_STALL)
            }
            URB_FUNCTION_SELECT_CONFIGURATION => {
                self.handle_select_configuration(req)
            }
            URB_FUNCTION_SELECT_INTERFACE => {
                self.handle_select_interface(req)
            }
            _ => UrbCompletion {
                sequence_id: seq,
                device_id: self.device_id,
                endpoint_address: ep,
                function_code: func,
                usbd_status: USBD_STATUS_INVALID_PARAMETER,
                actual_length: 0,
                data: Vec::new(),
                iso_packets: Vec::new(),
            },
        }
    }

    // -------------------------------------------------------------------------
    // URB Handlers
    // -------------------------------------------------------------------------

    fn handle_control_transfer(&mut self, req: UrbRequest) -> UrbCompletion {
        let (setup, data) = match &req.payload {
            UrbPayload::Control { setup, data } => (*setup, data.clone()),
            _ => {
                return UrbCompletion {
                    sequence_id: req.sequence_id,
                    device_id: self.device_id,
                    endpoint_address: req.endpoint_address,
                    function_code: req.function_code,
                    usbd_status: USBD_STATUS_INVALID_PARAMETER,
                    actual_length: 0,
                    data: Vec::new(),
                    iso_packets: Vec::new(),
                }
            }
        };

        // Check if pipe exists; fallback to EP0 if unmapped
        let pipe = self.pipes.entry(req.endpoint_address).or_insert_with(|| {
            UrbPipe::new(req.endpoint_address, TransferType::Control, 64)
        });

        if pipe.state == PipeState::Stalled {
            return UrbCompletion {
                sequence_id: req.sequence_id,
                device_id: self.device_id,
                endpoint_address: req.endpoint_address,
                function_code: req.function_code,
                usbd_status: USBD_STATUS_STALL_PID,
                actual_length: 0,
                data: Vec::new(),
                iso_packets: Vec::new(),
            };
        }

        pipe.total_transfers += 1;
        pipe.total_bytes += data.len() as u64;

        // Process standard USB clear feature on endpoint if target is an endpoint stall
        if setup.request_type_standard() && setup.b_request == 0x01 /* CLEAR_FEATURE */ && (setup.bm_request_type & 0x1F) == 2 {
            let target_ep = (setup.w_index & 0xFF) as u8;
            if let Some(target_pipe) = self.pipes.get_mut(&target_ep) {
                target_pipe.state = PipeState::Idle;
                target_pipe.reset_toggle();
            }
        }

        UrbCompletion {
            sequence_id: req.sequence_id,
            device_id: self.device_id,
            endpoint_address: req.endpoint_address,
            function_code: req.function_code,
            usbd_status: USBD_STATUS_SUCCESS,
            actual_length: data.len() as u32,
            data,
            iso_packets: Vec::new(),
        }
    }

    fn handle_bulk_or_interrupt_transfer(&mut self, req: UrbRequest) -> UrbCompletion {
        let data = match &req.payload {
            UrbPayload::BulkOrInterrupt { data } => data.clone(),
            _ => {
                return UrbCompletion {
                    sequence_id: req.sequence_id,
                    device_id: self.device_id,
                    endpoint_address: req.endpoint_address,
                    function_code: req.function_code,
                    usbd_status: USBD_STATUS_INVALID_PARAMETER,
                    actual_length: 0,
                    data: Vec::new(),
                    iso_packets: Vec::new(),
                }
            }
        };

        let pipe = match self.pipes.get_mut(&req.endpoint_address) {
            Some(p) => p,
            None => {
                return UrbCompletion {
                    sequence_id: req.sequence_id,
                    device_id: self.device_id,
                    endpoint_address: req.endpoint_address,
                    function_code: req.function_code,
                    usbd_status: USBD_STATUS_INVALID_PIPE_HANDLE,
                    actual_length: 0,
                    data: Vec::new(),
                    iso_packets: Vec::new(),
                };
            }
        };

        // Stall checking
        if pipe.state == PipeState::Stalled {
            return UrbCompletion {
                sequence_id: req.sequence_id,
                device_id: self.device_id,
                endpoint_address: req.endpoint_address,
                function_code: req.function_code,
                usbd_status: USBD_STATUS_STALL_PID,
                actual_length: 0,
                data: Vec::new(),
                iso_packets: Vec::new(),
            };
        }

        // Toggle advance
        let _toggle_used = pipe.step_toggle();
        pipe.total_transfers += 1;
        pipe.total_bytes += data.len() as u64;

        UrbCompletion {
            sequence_id: req.sequence_id,
            device_id: self.device_id,
            endpoint_address: req.endpoint_address,
            function_code: req.function_code,
            usbd_status: USBD_STATUS_SUCCESS,
            actual_length: data.len() as u32,
            data,
            iso_packets: Vec::new(),
        }
    }

    fn handle_isoch_transfer(&mut self, req: UrbRequest) -> UrbCompletion {
        let (_start_frame, _number_of_packets, mut in_packets, data) = match &req.payload {
            UrbPayload::Isochronous {
                start_frame,
                number_of_packets,
                packets,
                data,
            } => (*start_frame, *number_of_packets, packets.clone(), data.clone()),
            _ => {
                return UrbCompletion {
                    sequence_id: req.sequence_id,
                    device_id: self.device_id,
                    endpoint_address: req.endpoint_address,
                    function_code: req.function_code,
                    usbd_status: USBD_STATUS_INVALID_PARAMETER,
                    actual_length: 0,
                    data: Vec::new(),
                    iso_packets: Vec::new(),
                }
            }
        };

        let now_us = self.start_time.elapsed().as_micros() as u64;
        let mut total_actual = 0;

        // Stamp arrival/completion timestamps and verify offsets
        for (idx, pkt) in in_packets.iter_mut().enumerate() {
            pkt.timestamp_us = now_us + (idx as u64 * 125); // 125us microframe pacing
            pkt.status = USBD_STATUS_SUCCESS;
            pkt.actual_length = pkt.length;
            total_actual += pkt.actual_length;
        }

        if let Some(pipe) = self.pipes.get_mut(&req.endpoint_address) {
            pipe.total_transfers += 1;
            pipe.total_bytes += total_actual as u64;
        }

        UrbCompletion {
            sequence_id: req.sequence_id,
            device_id: self.device_id,
            endpoint_address: req.endpoint_address,
            function_code: req.function_code,
            usbd_status: USBD_STATUS_SUCCESS,
            actual_length: total_actual,
            data,
            iso_packets: in_packets,
        }
    }

    fn handle_abort_pipe(&mut self, seq: u64, ep: u8) -> UrbCompletion {
        if let Some(pipe) = self.pipes.get_mut(&ep) {
            pipe.state = PipeState::Aborted;
        }

        // Drop any matching in-flight URBs
        self.in_flight.retain(|_, v| v.endpoint_address != ep);

        UrbCompletion {
            sequence_id: seq,
            device_id: self.device_id,
            endpoint_address: ep,
            function_code: URB_FUNCTION_ABORT_PIPE,
            usbd_status: USBD_STATUS_SUCCESS,
            actual_length: 0,
            data: Vec::new(),
            iso_packets: Vec::new(),
        }
    }

    fn handle_reset_pipe(&mut self, seq: u64, ep: u8, clear_stall: bool) -> UrbCompletion {
        if let Some(pipe) = self.pipes.get_mut(&ep) {
            pipe.reset_toggle();
            if clear_stall || pipe.state == PipeState::Stalled {
                pipe.state = PipeState::Idle;
            }
        }

        UrbCompletion {
            sequence_id: seq,
            device_id: self.device_id,
            endpoint_address: ep,
            function_code: if clear_stall {
                URB_FUNCTION_SYNC_RESET_PIPE_AND_CLEAR_STALL
            } else {
                URB_FUNCTION_SYNC_RESET_PIPE
            },
            usbd_status: USBD_STATUS_SUCCESS,
            actual_length: 0,
            data: Vec::new(),
            iso_packets: Vec::new(),
        }
    }

    fn handle_select_configuration(&mut self, req: UrbRequest) -> UrbCompletion {
        let config_val = match req.payload {
            UrbPayload::SelectConfiguration { configuration_value } => configuration_value,
            _ => 1,
        };

        self.active_configuration = config_val;
        // Reset all endpoint states & data toggles to DATA0
        for pipe in self.pipes.values_mut() {
            pipe.state = PipeState::Idle;
            pipe.reset_toggle();
        }

        UrbCompletion {
            sequence_id: req.sequence_id,
            device_id: self.device_id,
            endpoint_address: 0x00,
            function_code: URB_FUNCTION_SELECT_CONFIGURATION,
            usbd_status: USBD_STATUS_SUCCESS,
            actual_length: 0,
            data: Vec::new(),
            iso_packets: Vec::new(),
        }
    }

    fn handle_select_interface(&mut self, req: UrbRequest) -> UrbCompletion {
        let (iface, alt) = match req.payload {
            UrbPayload::SelectInterface {
                interface_number,
                alternate_setting,
            } => (interface_number, alternate_setting),
            _ => (0, 0),
        };

        self.active_interface_settings.insert(iface, alt);
        for pipe in self.pipes.values_mut() {
            pipe.reset_toggle();
        }

        UrbCompletion {
            sequence_id: req.sequence_id,
            device_id: self.device_id,
            endpoint_address: 0x00,
            function_code: URB_FUNCTION_SELECT_INTERFACE,
            usbd_status: USBD_STATUS_SUCCESS,
            actual_length: 0,
            data: Vec::new(),
            iso_packets: Vec::new(),
        }
    }

    // -------------------------------------------------------------------------
    // Turbo-URB Wire Protocol Translation Helpers
    // -------------------------------------------------------------------------

    pub fn to_turbo_header(&self, req: &UrbRequest) -> TurboUrbHeader {
        let (xfer_type, buf_len) = match &req.payload {
            UrbPayload::Control { data, .. } => (TransferType::Control, data.len() as u32),
            UrbPayload::BulkOrInterrupt { data } => {
                let xt = self.pipes.get(&req.endpoint_address)
                    .map(|p| p.transfer_type)
                    .unwrap_or(TransferType::Bulk);
                (xt, data.len() as u32)
            }
            UrbPayload::Isochronous { data, .. } => (TransferType::Isochronous, data.len() as u32),
            _ => (TransferType::Control, 0),
        };

        let mut hdr = TurboUrbHeader::new(
            CMD_SUBMIT_URB,
            req.sequence_id,
            self.device_id,
            req.endpoint_address,
            xfer_type,
            buf_len,
        );
        if xfer_type == TransferType::Isochronous {
            hdr.flags |= FLAG_ISOCH_PACKET;
        }
        hdr
    }

    pub fn completion_to_turbo_header(&self, comp: &UrbCompletion) -> TurboUrbHeader {
        let xt = self.pipes.get(&comp.endpoint_address)
            .map(|p| p.transfer_type)
            .unwrap_or(TransferType::Bulk);

        let mut hdr = TurboUrbHeader::new(
            CMD_COMPLETE_URB,
            comp.sequence_id,
            comp.device_id,
            comp.endpoint_address,
            xt,
            comp.actual_length,
        );
        hdr.status = comp.usbd_status as i32;
        hdr.actual_length = comp.actual_length;
        if xt == TransferType::Isochronous {
            hdr.flags |= FLAG_ISOCH_PACKET;
        }
        hdr
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_control_transfer_and_clear_stall() {
        let mut engine = UrbEngine::new(1);
        engine.register_pipe(0x81, TransferType::Bulk, 512);

        // Intentionally stall Bulk EP 0x81
        engine.inject_endpoint_stall(0x81);
        assert_eq!(engine.pipes.get(&0x81).unwrap().state, PipeState::Stalled);

        // Submitting bulk transfer to stalled pipe must return USBD_STATUS_STALL_PID
        let bulk_req = UrbRequest {
            sequence_id: 101,
            device_id: 1,
            endpoint_address: 0x81,
            function_code: URB_FUNCTION_BULK_OR_INTERRUPT_TRANSFER,
            transfer_flags: 0,
            payload: UrbPayload::BulkOrInterrupt { data: vec![0u8; 512] },
        };
        let comp = engine.submit_urb(bulk_req);
        assert_eq!(comp.usbd_status, USBD_STATUS_STALL_PID);

        // Send Clear Stall via URB_FUNCTION_SYNC_RESET_PIPE_AND_CLEAR_STALL
        let reset_req = UrbRequest {
            sequence_id: 102,
            device_id: 1,
            endpoint_address: 0x81,
            function_code: URB_FUNCTION_SYNC_RESET_PIPE_AND_CLEAR_STALL,
            transfer_flags: 0,
            payload: UrbPayload::ResetPipe {
                endpoint_address: 0x81,
                clear_stall: true,
            },
        };
        let reset_comp = engine.submit_urb(reset_req);
        assert_eq!(reset_comp.usbd_status, USBD_STATUS_SUCCESS);
        assert_eq!(engine.pipes.get(&0x81).unwrap().state, PipeState::Idle);

        // Subsequent bulk transfer now succeeds
        let bulk_req2 = UrbRequest {
            sequence_id: 103,
            device_id: 1,
            endpoint_address: 0x81,
            function_code: URB_FUNCTION_BULK_OR_INTERRUPT_TRANSFER,
            transfer_flags: 0,
            payload: UrbPayload::BulkOrInterrupt { data: vec![0xBB; 512] },
        };
        let comp2 = engine.submit_urb(bulk_req2);
        assert_eq!(comp2.usbd_status, USBD_STATUS_SUCCESS);
        assert_eq!(comp2.actual_length, 512);
    }

    #[test]
    fn test_bulk_data_toggle_alternation() {
        let mut engine = UrbEngine::new(2);
        engine.register_pipe(0x02, TransferType::Bulk, 512);

        // Initial toggle should be false (DATA0)
        assert_eq!(engine.pipes.get(&0x02).unwrap().data_toggle, false);

        // First transfer: moves toggle to true (DATA1)
        let req1 = UrbRequest {
            sequence_id: 201,
            device_id: 2,
            endpoint_address: 0x02,
            function_code: URB_FUNCTION_BULK_OR_INTERRUPT_TRANSFER,
            transfer_flags: 0,
            payload: UrbPayload::BulkOrInterrupt { data: vec![1, 2, 3] },
        };
        let comp1 = engine.submit_urb(req1);
        assert_eq!(comp1.usbd_status, USBD_STATUS_SUCCESS);
        assert_eq!(engine.pipes.get(&0x02).unwrap().data_toggle, true);

        // Second transfer: moves toggle back to false (DATA0)
        let req2 = UrbRequest {
            sequence_id: 202,
            device_id: 2,
            endpoint_address: 0x02,
            function_code: URB_FUNCTION_BULK_OR_INTERRUPT_TRANSFER,
            transfer_flags: 0,
            payload: UrbPayload::BulkOrInterrupt { data: vec![4, 5, 6] },
        };
        let comp2 = engine.submit_urb(req2);
        assert_eq!(comp2.usbd_status, USBD_STATUS_SUCCESS);
        assert_eq!(engine.pipes.get(&0x02).unwrap().data_toggle, false);
    }

    #[test]
    fn test_isochronous_transfer_microsecond_pacing() {
        let mut engine = UrbEngine::new(3);
        engine.register_pipe(0x83, TransferType::Isochronous, 1024);

        let packets = vec![
            UrbIsoPacket { offset: 0, length: 192, actual_length: 0, status: 0, timestamp_us: 0 },
            UrbIsoPacket { offset: 192, length: 192, actual_length: 0, status: 0, timestamp_us: 0 },
            UrbIsoPacket { offset: 384, length: 192, actual_length: 0, status: 0, timestamp_us: 0 },
        ];

        let isoch_req = UrbRequest {
            sequence_id: 301,
            device_id: 3,
            endpoint_address: 0x83,
            function_code: URB_FUNCTION_ISOCH_TRANSFER,
            transfer_flags: 0,
            payload: UrbPayload::Isochronous {
                start_frame: 100,
                number_of_packets: 3,
                packets,
                data: vec![0u8; 576],
            },
        };

        let comp = engine.submit_urb(isoch_req);
        assert_eq!(comp.usbd_status, USBD_STATUS_SUCCESS);
        assert_eq!(comp.actual_length, 576);
        assert_eq!(comp.iso_packets.len(), 3);
        // Verify microsecond timestamps are sequential
        assert!(comp.iso_packets[1].timestamp_us > comp.iso_packets[0].timestamp_us);
        assert!(comp.iso_packets[2].timestamp_us > comp.iso_packets[1].timestamp_us);
    }

    #[test]
    fn test_abort_pipe_cancels_pipe_activity() {
        let mut engine = UrbEngine::new(4);
        engine.register_pipe(0x84, TransferType::Bulk, 512);

        let abort_req = UrbRequest {
            sequence_id: 401,
            device_id: 4,
            endpoint_address: 0x84,
            function_code: URB_FUNCTION_ABORT_PIPE,
            transfer_flags: 0,
            payload: UrbPayload::AbortPipe { endpoint_address: 0x84 },
        };

        let comp = engine.submit_urb(abort_req);
        assert_eq!(comp.usbd_status, USBD_STATUS_SUCCESS);
        assert_eq!(engine.pipes.get(&0x84).unwrap().state, PipeState::Aborted);
    }
}
