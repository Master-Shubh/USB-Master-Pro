//! End-to-End Virtual USB Loopback Channel & Biometric Verification Engine
//! Simulates a physical USB biometric peripheral (Mantra MFS100 / Morpho MSO1300)
//! and validates end-to-end byte-for-byte fidelity across the Turbo-URB pipeline.

use crate::buffer_pool::BufferPool;
use crate::compression::TurboCompressor;
use crate::device_reset::DeviceResetController;
use crate::protocol::{
    TransferType, TurboUrbHeader, CMD_COMPLETE_URB, FLAG_COMPRESSED_ZSTD,
};
use crate::urb_engine::{
    UrbEngine, UrbPayload, UrbRequest, UsbSetupPacket,
    URB_FUNCTION_SYNC_RESET_PIPE_AND_CLEAR_STALL,
};
use crate::usb_descriptors::{
    UsbConfigurationDescriptor, UsbDeviceDescriptor, UsbEndpointDescriptor, UsbInterfaceDescriptor,
    UsbParsedConfiguration, UsbParsedEndpoint, UsbParsedInterface, DESC_TYPE_CONFIGURATION,
    DESC_TYPE_DEVICE, DESC_TYPE_ENDPOINT, DESC_TYPE_INTERFACE,
};
use std::sync::{Arc, Mutex};

pub const MANTRA_VID: u16 = 0x27D0;
pub const MANTRA_PID: u16 = 0x0041;
pub const BIOMETRIC_FRAME_WIDTH: usize = 512;
pub const BIOMETRIC_FRAME_HEIGHT: usize = 512;
pub const BIOMETRIC_FRAME_SIZE: usize = BIOMETRIC_FRAME_WIDTH * BIOMETRIC_FRAME_HEIGHT; // 262,144 bytes (256 KB)

// -----------------------------------------------------------------------------
// 1. Simulated Mantra MFS100 Biometric Peripheral
// -----------------------------------------------------------------------------
pub struct SimulatedBiometricDevice {
    pub device_id: u32,
    pub vid: u16,
    pub pid: u16,
    pub reset_controller: DeviceResetController,
    pub urb_engine: UrbEngine,
    pub current_frame: Vec<u8>,
    pub frame_offset: usize,
    pub total_frames_sent: u64,
}

impl SimulatedBiometricDevice {
    pub fn new_mantra_mfs100(device_id: u32) -> Self {
        let mut dev = Self {
            device_id,
            vid: MANTRA_VID,
            pid: MANTRA_PID,
            reset_controller: DeviceResetController::new(device_id),
            urb_engine: UrbEngine::new(device_id),
            current_frame: Self::generate_synthetic_fingerprint_frame(1),
            frame_offset: 0,
            total_frames_sent: 0,
        };

        // Initialize standard USB descriptors
        let config = Self::build_mantra_configuration_tree();
        dev.urb_engine.configure_from_descriptors(&config);

        // Power-on attach
        let _ = dev.reset_controller.attach_initial();
        dev
    }

    /// Builds realistic Mantra MFS100 USB descriptors
    pub fn build_mantra_configuration_tree() -> UsbParsedConfiguration {
        UsbParsedConfiguration {
            descriptor: UsbConfigurationDescriptor {
                b_length: 9,
                b_descriptor_type: DESC_TYPE_CONFIGURATION,
                w_total_length: 32,
                b_num_interfaces: 1,
                b_configuration_value: 1,
                i_configuration: 0,
                bm_attributes: 0x80, // Bus powered
                b_max_power: 50,     // 100mA
            },
            iad_list: Vec::new(),
            interfaces: vec![UsbParsedInterface {
                descriptor: UsbInterfaceDescriptor {
                    b_length: 9,
                    b_descriptor_type: DESC_TYPE_INTERFACE,
                    b_interface_number: 0,
                    b_alternate_setting: 0,
                    b_num_endpoints: 2,
                    b_interface_class: 0xFF, // Vendor Specific Class
                    b_interface_sub_class: 0x00,
                    b_interface_protocol: 0x00,
                    i_interface: 0,
                },
                hid_descriptor: None,
                endpoints: vec![
                    UsbParsedEndpoint {
                        descriptor: UsbEndpointDescriptor {
                            b_length: 7,
                            b_descriptor_type: DESC_TYPE_ENDPOINT,
                            b_endpoint_address: 0x81, // EP1 IN (Bulk Frame Data)
                            bm_attributes: 0x02,       // Bulk
                            w_max_packet_size: 512,
                            b_interval: 0,
                        },
                        ss_companion: None,
                        extra: Vec::new(),
                    },
                    UsbParsedEndpoint {
                        descriptor: UsbEndpointDescriptor {
                            b_length: 7,
                            b_descriptor_type: DESC_TYPE_ENDPOINT,
                            b_endpoint_address: 0x01, // EP1 OUT (Command Dispatch)
                            bm_attributes: 0x02,       // Bulk
                            w_max_packet_size: 512,
                            b_interval: 0,
                        },
                        ss_companion: None,
                        extra: Vec::new(),
                    },
                ],
                class_specific: Vec::new(),
                extra: Vec::new(),
            }],
            extra: Vec::new(),
        }
    }

    /// Generates synthetic 256 KB 8-bit optical grayscale fingerprint image
    pub fn generate_synthetic_fingerprint_frame(seed: u8) -> Vec<u8> {
        let mut frame = vec![0u8; BIOMETRIC_FRAME_SIZE];
        for y in 0..BIOMETRIC_FRAME_HEIGHT {
            for x in 0..BIOMETRIC_FRAME_WIDTH {
                let idx = y * BIOMETRIC_FRAME_WIDTH + x;
                // Synthetic optical ridge-valley sinusoidal pattern with core pattern
                let dist_from_center = (((x as f32 - 256.0).powi(2) + (y as f32 - 256.0).powi(2)).sqrt()) as u32;
                let ridge = ((dist_from_center * 16 + (seed as u32 * 7)) % 256) as u8;
                frame[idx] = if ridge > 128 { 0xF0 } else { 0x10 };
            }
        }
        frame
    }

    /// Processes an incoming raw Turbo-URB packet on the simulated device
    pub fn process_urb(&mut self, header: &TurboUrbHeader, payload: &[u8]) -> (TurboUrbHeader, Vec<u8>) {
        let mut resp_header = header.clone();
        resp_header.command = CMD_COMPLETE_URB;

        // Check reset state acceptance
        if let Err(err_code) = self.reset_controller.can_accept_urb(header.endpoint_address, header.transfer_type) {
            resp_header.status = err_code;
            resp_header.actual_length = 0;
            return (resp_header, Vec::new());
        }

        match header.transfer_type {
            TransferType::Control => {
                // Parse 8-byte setup packet
                let setup = match UsbSetupPacket::parse(payload) {
                    Ok(s) => s,
                    Err(_) => {
                        resp_header.status = -1;
                        return (resp_header, Vec::new());
                    }
                };

                let mut out_data = Vec::new();
                if setup.request_type_standard() {
                    match setup.b_request {
                        0x06 /* GET_DESCRIPTOR */ => {
                            let desc_type = (setup.w_value >> 8) as u8;
                            if desc_type == DESC_TYPE_DEVICE {
                                let dev_desc = UsbDeviceDescriptor {
                                    b_length: 18,
                                    b_descriptor_type: DESC_TYPE_DEVICE,
                                    bcd_usb: 0x0200,
                                    b_device_class: 0,
                                    b_device_sub_class: 0,
                                    b_device_protocol: 0,
                                    b_max_packet_size0: 64,
                                    id_vendor: self.vid,
                                    id_product: self.pid,
                                    bcd_device: 0x0100,
                                    i_manufacturer: 1,
                                    i_product: 2,
                                    i_serial_number: 3,
                                    b_num_configurations: 1,
                                };
                                out_data = dev_desc.to_bytes();
                            } else if desc_type == DESC_TYPE_CONFIGURATION {
                                let config_tree = Self::build_mantra_configuration_tree();
                                out_data = config_tree.to_bytes();
                            }
                        }
                        0x05 /* SET_ADDRESS */ => {
                            let new_addr = (setup.w_value & 0xFF) as u8;
                            let _ = self.reset_controller.set_address(new_addr);
                        }
                        0x09 /* SET_CONFIGURATION */ => {
                            let config_val = (setup.w_value & 0xFF) as u8;
                            let _ = self.reset_controller.set_configuration(config_val);
                            let _ = self.reset_controller.start_streaming();
                            self.frame_offset = 0;
                        }
                        0x01 /* CLEAR_FEATURE (ENDPOINT_HALT) */ => {
                            let target_ep = (setup.w_index & 0xFF) as u8;
                            let _ = self.reset_controller.trigger_soft_reset(target_ep);
                            let _ = self.urb_engine.submit_urb(UrbRequest {
                                sequence_id: header.sequence_id,
                                device_id: self.device_id,
                                endpoint_address: target_ep,
                                function_code: URB_FUNCTION_SYNC_RESET_PIPE_AND_CLEAR_STALL,
                                transfer_flags: 0,
                                payload: UrbPayload::ResetPipe {
                                    endpoint_address: target_ep,
                                    clear_stall: true,
                                },
                            });
                        }
                        _ => {}
                    }
                } else if setup.request_type_vendor() {
                    // Simulated Mantra vendor commands (e.g. 0x10 = Start Frame Stream)
                    match setup.b_request {
                        0x10 => {
                            self.frame_offset = 0;
                            out_data = vec![0xAA, 0x55, 0x01]; // Sensor ACK
                        }
                        _ => {
                            out_data = vec![0xAA, 0x55];
                        }
                    }
                }

                resp_header.status = 0;
                resp_header.actual_length = out_data.len() as u32;
                (resp_header, out_data)
            }
            TransferType::Bulk => {
                if (header.endpoint_address & 0x80) != 0 {
                    // Bulk IN (Read fingerprint frame chunks)
                    let requested_len = header.buffer_length as usize;
                    let remaining = self.current_frame.len().saturating_sub(self.frame_offset);
                    let to_send = requested_len.min(remaining);

                    let slice = &self.current_frame[self.frame_offset..self.frame_offset + to_send];
                    let chunk = slice.to_vec();
                    self.frame_offset += to_send;

                    if self.frame_offset >= self.current_frame.len() {
                        self.total_frames_sent += 1;
                        self.frame_offset = 0; // Wrap around for next frame
                    }

                    resp_header.status = 0;
                    resp_header.actual_length = chunk.len() as u32;
                    (resp_header, chunk)
                } else {
                    // Bulk OUT (Write sensor command packet)
                    resp_header.status = 0;
                    resp_header.actual_length = payload.len() as u32;
                    (resp_header, Vec::new())
                }
            }
            _ => {
                resp_header.status = 0;
                resp_header.actual_length = 0;
                (resp_header, Vec::new())
            }
        }
    }
}

// -----------------------------------------------------------------------------
// 2. High-Fidelity Turbo-URB Simulated Loopback Pipeline
// -----------------------------------------------------------------------------
pub struct VirtualLoopbackPipeline {
    device: Arc<Mutex<SimulatedBiometricDevice>>,
    pool: BufferPool,
}

impl VirtualLoopbackPipeline {
    pub fn new(device_id: u32) -> Self {
        let device = Arc::new(Mutex::new(SimulatedBiometricDevice::new_mantra_mfs100(device_id)));
        let pool = BufferPool::new(32, 65536);
        Self { device, pool }
    }

    /// Sends a raw URB request across the Turbo-URB pipeline and returns the verified response
    pub fn transfer(&self, req_header: &TurboUrbHeader, payload: &[u8]) -> (TurboUrbHeader, Vec<u8>) {
        // 1. Client Serialization & Optional ZSTD Compression
        let (wire_payload, is_compressed) = TurboCompressor::compress_if_beneficial(payload);
        let mut send_header = req_header.clone();
        if is_compressed {
            send_header.flags |= FLAG_COMPRESSED_ZSTD;
            send_header.buffer_length = wire_payload.len() as u32;
        }

        // Simulate wire transport: serialize and deserialize header
        let wire_header_bytes = send_header.serialize();
        let parsed_header = TurboUrbHeader::deserialize(&wire_header_bytes)
            .expect("Failed to deserialize wire header");

        // 2. Server Decompression
        let server_payload = if (parsed_header.flags & FLAG_COMPRESSED_ZSTD) != 0 {
            TurboCompressor::decompress(&wire_payload).expect("Server failed decompress")
        } else {
            wire_payload
        };

        // 3. Simulated Device Execution
        let (mut resp_header, resp_data) = {
            let mut dev = self.device.lock().unwrap();
            dev.process_urb(&parsed_header, &server_payload)
        };

        // 4. Server Response Compression & Wire Framing
        let (wire_resp_data, resp_compressed) = TurboCompressor::compress_if_beneficial(&resp_data);
        if resp_compressed {
            resp_header.flags |= FLAG_COMPRESSED_ZSTD;
        }

        let resp_header_bytes = resp_header.serialize();
        let client_recv_header = TurboUrbHeader::deserialize(&resp_header_bytes)
            .expect("Client failed deserialize response header");

        // 5. Client Decompression & Fidelity Verification
        let client_final_data = if (client_recv_header.flags & FLAG_COMPRESSED_ZSTD) != 0 {
            TurboCompressor::decompress(&wire_resp_data).expect("Client failed decompress")
        } else {
            wire_resp_data
        };

        (client_recv_header, client_final_data)
    }

    pub fn device(&self) -> Arc<Mutex<SimulatedBiometricDevice>> {
        Arc::clone(&self.device)
    }

    pub fn pool(&self) -> &BufferPool {
        &self.pool
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::device_reset::DeviceOperationalState;
    use crate::protocol::CMD_SUBMIT_URB;
    use sha2::{Digest, Sha256};
    use std::time::Instant;

    #[test]
    fn test_loopback_control_enumeration_handshake() {
        let pipeline = VirtualLoopbackPipeline::new(1);

        // 1. Get Device Descriptor (18 bytes)
        let mut setup = UsbSetupPacket::new(0x80, 0x06, 0x0100, 0, 18);
        let req = TurboUrbHeader::new(CMD_SUBMIT_URB, 1, 1, 0x00, TransferType::Control, 8);
        let (resp_hdr, data) = pipeline.transfer(&req, &setup.to_bytes());
        assert_eq!(resp_hdr.status, 0);
        assert_eq!(data.len(), 18);

        let dev_desc = UsbDeviceDescriptor::parse(&data).expect("Must parse MFS100 descriptor");
        assert_eq!(dev_desc.id_vendor, MANTRA_VID);
        assert_eq!(dev_desc.id_product, MANTRA_PID);

        // 2. Set Address 5
        setup = UsbSetupPacket::new(0x00, 0x05, 5, 0, 0);
        let (addr_resp, _) = pipeline.transfer(&req, &setup.to_bytes());
        assert_eq!(addr_resp.status, 0);

        // 3. Set Configuration 1
        setup = UsbSetupPacket::new(0x00, 0x09, 1, 0, 0);
        let (cfg_resp, _) = pipeline.transfer(&req, &setup.to_bytes());
        assert_eq!(cfg_resp.status, 0);

        // Verify device state is streaming
        let dev = pipeline.device.lock().unwrap();
        assert_eq!(dev.reset_controller.state, DeviceOperationalState::ActiveStreaming);
    }

    #[test]
    fn test_loopback_high_throughput_biometric_frame_fidelity() {
        let pipeline = VirtualLoopbackPipeline::new(2);

        // Enumerate first
        let setup_cfg = UsbSetupPacket::new(0x00, 0x09, 1, 0, 0);
        let req_cfg = TurboUrbHeader::new(CMD_SUBMIT_URB, 1, 2, 0x00, TransferType::Control, 8);
        pipeline.transfer(&req_cfg, &setup_cfg.to_bytes());

        // Calculate expected frame SHA-256 hash directly from device's generated pattern
        let expected_frame = SimulatedBiometricDevice::generate_synthetic_fingerprint_frame(1);
        let expected_hash = Sha256::digest(&expected_frame);

        // Read entire 262,144 bytes in 4096-byte Bulk IN URBs
        const CHUNK_SIZE: usize = 4096;
        let total_chunks = BIOMETRIC_FRAME_SIZE / CHUNK_SIZE;
        let mut reassembled_frame = Vec::with_capacity(BIOMETRIC_FRAME_SIZE);

        let start_time = Instant::now();

        for seq in 0..total_chunks {
            let urb_req = TurboUrbHeader::new(
                CMD_SUBMIT_URB,
                1000 + seq as u64,
                2,
                0x81, // EP1 IN
                TransferType::Bulk,
                CHUNK_SIZE as u32,
            );

            let (resp_hdr, chunk) = pipeline.transfer(&urb_req, &[]);
            assert_eq!(resp_hdr.status, 0);
            assert_eq!(chunk.len(), CHUNK_SIZE);
            reassembled_frame.extend_from_slice(&chunk);
        }

        let elapsed = start_time.elapsed();
        let actual_hash = Sha256::digest(&reassembled_frame);

        // Assert 100% byte-for-byte fidelity
        assert_eq!(
            expected_hash, actual_hash,
            "Reassembled biometric frame hash mismatch: data corruption detected!"
        );
        assert_eq!(reassembled_frame, expected_frame);

        let throughput_mb = (BIOMETRIC_FRAME_SIZE as f64 / (1024.0 * 1024.0)) / elapsed.as_secs_f64();
        println!(
            "⚡ Biometric Frame Loopback: 256 KB transfer in {:.2?} ({:.1} MB/s)",
            elapsed, throughput_mb
        );
    }

    #[test]
    fn test_loopback_stall_injection_and_soft_reset_recovery() {
        let pipeline = VirtualLoopbackPipeline::new(3);

        // Enumerate
        let setup_cfg = UsbSetupPacket::new(0x00, 0x09, 1, 0, 0);
        let req_cfg = TurboUrbHeader::new(CMD_SUBMIT_URB, 1, 3, 0x00, TransferType::Control, 8);
        pipeline.transfer(&req_cfg, &setup_cfg.to_bytes());

        // 1. Initial read succeeds
        let req_read = TurboUrbHeader::new(CMD_SUBMIT_URB, 2, 3, 0x81, TransferType::Bulk, 512);
        let (resp1, data1) = pipeline.transfer(&req_read, &[]);
        assert_eq!(resp1.status, 0);
        assert_eq!(data1.len(), 512);

        // 2. Inject STALL on EP 0x81
        {
            let mut dev = pipeline.device.lock().unwrap();
            dev.reset_controller.stalled_endpoints.insert(0x81);
        }

        // 3. Next bulk transfer must fail with EPIPE / STALL
        let (resp_stalled, _) = pipeline.transfer(&req_read, &[]);
        assert_eq!(resp_stalled.status, crate::device_reset::EPIPE);

        // 4. Client issues Soft Reset: CLEAR_FEATURE(ENDPOINT_HALT) on EP 0x81
        let clear_stall_setup = UsbSetupPacket::new(0x02, 0x01, 0, 0x0081, 0);
        let req_ctrl = TurboUrbHeader::new(CMD_SUBMIT_URB, 3, 3, 0x00, TransferType::Control, 8);
        let (clear_resp, _) = pipeline.transfer(&req_ctrl, &clear_stall_setup.to_bytes());
        assert_eq!(clear_resp.status, 0);

        // 5. Subsequent bulk transfer recovers cleanly!
        let (resp_recovered, data_recovered) = pipeline.transfer(&req_read, &[]);
        assert_eq!(resp_recovered.status, 0);
        assert_eq!(data_recovered.len(), 512);
    }
}
