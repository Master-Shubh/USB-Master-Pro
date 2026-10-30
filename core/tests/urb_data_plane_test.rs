//! USB Master Pro — High-Performance URB Data Plane Integration Test Suite
//! Validates:
//! 1. FilterDriverClient lifecycle (open, start_capture, fetch_urb, complete_urb, stop_capture)
//! 2. Full loopback URB pump pipeline (VHCI pump -> Wire framing -> Network pump -> VHCI completion)
//! 3. 64 KB boundary enforcement (protection against buffer overflow & oversized single URBs)
//! 4. Adaptive backoff timing and queue responsiveness

use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{mpsc, watch};
use usb_master_core::daemon::{run_network_to_vhci_pump, run_vhci_to_network_pump};
use usb_master_core::driver_client::{
    AnyUsbCompleteRequest, AnyUsbUrbMessage, DriverClientError, FilterDriverClient,
    VhciDriverClient, ANYUSB_MAX_URB_BUFFER_SIZE,
};
use usb_master_core::protocol::{
    TransferType, TurboUrbHeader, CMD_COMPLETE_URB, CMD_SUBMIT_URB,
};

#[test]
fn test_filter_driver_client_lifecycle() {
    let client = FilterDriverClient::open().expect("FilterDriverClient open failed");
    let test_dev_id: u32 = 101;

    // 1. Start capture
    client
        .start_capture(test_dev_id)
        .expect("start_capture failed");
    assert!(client.is_capturing(test_dev_id));

    // 2. Fetch on empty queue should return None
    let empty_fetch = client.fetch_urb().expect("fetch_urb failed on empty queue");
    assert!(empty_fetch.is_none());

    // 3. Queue simulated URB
    let mut sim_msg = AnyUsbUrbMessage::default();
    sim_msg.device_id = test_dev_id;
    sim_msg.sequence_id = 9001;
    sim_msg.endpoint_address = 0x81;
    sim_msg.transfer_type = 2; // Bulk
    sim_msg.transfer_buffer_length = 8;
    sim_msg.data[..8].copy_from_slice(b"CAPTURED");

    client.queue_simulated_urb(sim_msg);

    // 4. Fetch the queued URB
    let fetched = client
        .fetch_urb()
        .expect("fetch_urb failed")
        .expect("Expected queued URB");

    let dev_id = { fetched.device_id };
    let seq_id = { fetched.sequence_id };
    let ep = { fetched.endpoint_address };
    let xfer_len = { fetched.transfer_buffer_length };

    assert_eq!(dev_id, test_dev_id);
    assert_eq!(seq_id, 9001);
    assert_eq!(ep, 0x81);
    assert_eq!(xfer_len, 8);
    assert_eq!(&fetched.data[..8], b"CAPTURED");

    // 5. Complete URB
    let mut comp_req = AnyUsbCompleteRequest::default();
    comp_req.device_id = test_dev_id;
    comp_req.sequence_id = 9001;
    comp_req.status = 0;
    comp_req.actual_length = 4;
    comp_req.data[..4].copy_from_slice(b"ACK1");

    client
        .complete_urb(&comp_req)
        .expect("complete_urb failed");

    let completed_list = client.get_completed_urbs();
    assert_eq!(completed_list.len(), 1);
    let comp_dev = { completed_list[0].device_id };
    let comp_seq = { completed_list[0].sequence_id };
    let comp_status = { completed_list[0].status };
    let comp_len = { completed_list[0].actual_length };

    assert_eq!(comp_dev, test_dev_id);
    assert_eq!(comp_seq, 9001);
    assert_eq!(comp_status, 0);
    assert_eq!(comp_len, 4);
    assert_eq!(&completed_list[0].data[..4], b"ACK1");

    // 6. Stop capture
    client
        .stop_capture(test_dev_id)
        .expect("stop_capture failed");
    assert!(!client.is_capturing(test_dev_id));
}

#[tokio::test]
async fn test_full_loopback_urb_pump_pipeline() {
    let vhci_client = Arc::new(VhciDriverClient::open().expect("VhciDriverClient open failed"));
    let test_device_id: u32 = 42;

    let (shutdown_tx, shutdown_rx_vhci) = watch::channel(false);
    let shutdown_rx_net = shutdown_tx.subscribe();

    // Data channels
    let (vhci_to_net_tx, mut vhci_to_net_rx) = mpsc::channel::<Vec<u8>>(32);
    let (net_to_vhci_tx, net_to_vhci_rx) = mpsc::channel::<Vec<u8>>(32);

    // Spawn pump worker tasks
    let vhci_clone_1 = Arc::clone(&vhci_client);
    let vhci_to_net_task = tokio::spawn(async move {
        run_vhci_to_network_pump(vhci_clone_1, test_device_id, vhci_to_net_tx, shutdown_rx_vhci).await;
    });

    let vhci_clone_2 = Arc::clone(&vhci_client);
    let net_to_vhci_task = tokio::spawn(async move {
        run_network_to_vhci_pump(vhci_clone_2, net_to_vhci_rx, shutdown_rx_net).await;
    });

    // 1. Submit simulated URB into VHCI pending queue
    let mut submit_msg = AnyUsbUrbMessage::default();
    submit_msg.device_id = test_device_id;
    submit_msg.sequence_id = 777;
    submit_msg.endpoint_address = 0x02; // OUT endpoint
    submit_msg.transfer_type = 2; // Bulk
    submit_msg.transfer_buffer_length = 12;
    submit_msg.data[..12].copy_from_slice(b"HELLO-TURBO!");

    vhci_client.queue_simulated_urb(submit_msg);

    // 2. Receive frame produced by run_vhci_to_network_pump
    let frame = tokio::time::timeout(Duration::from_millis(500), vhci_to_net_rx.recv())
        .await
        .expect("Timeout waiting for URB frame from VHCI pump")
        .expect("Channel closed unexpectedly");

    assert!(frame.len() >= TurboUrbHeader::SIZE + 12);
    let parsed_hdr = TurboUrbHeader::deserialize_single_urb(&frame[..TurboUrbHeader::SIZE])
        .expect("Valid TurboUrbHeader");

    assert_eq!(parsed_hdr.command, CMD_SUBMIT_URB);
    assert_eq!(parsed_hdr.sequence_id, 777);
    assert_eq!(parsed_hdr.device_id, test_device_id);
    assert_eq!(parsed_hdr.endpoint_address, 0x02);
    assert_eq!(parsed_hdr.actual_length, 12);

    let payload = &frame[TurboUrbHeader::SIZE..TurboUrbHeader::SIZE + 12];
    assert_eq!(payload, b"HELLO-TURBO!");

    // 3. Emulate network completion response from remote device
    let mut comp_hdr = TurboUrbHeader::new(
        CMD_COMPLETE_URB,
        parsed_hdr.sequence_id,
        parsed_hdr.device_id,
        parsed_hdr.endpoint_address,
        TransferType::Bulk,
        8,
    );
    comp_hdr.status = 0;
    comp_hdr.actual_length = 8;

    let mut comp_frame = comp_hdr.serialize();
    comp_frame.extend_from_slice(b"RESP-OK!");

    net_to_vhci_tx
        .send(comp_frame)
        .await
        .expect("Send completion frame");

    // 4. Verify that run_network_to_vhci_pump completed the URB in VHCI driver client
    let mut completed = Vec::new();
    for _ in 0..20 {
        completed = vhci_client.get_completed_urbs();
        if !completed.is_empty() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(15)).await;
    }

    assert_eq!(completed.len(), 1, "Expected 1 completed URB in VHCI client");
    let c_dev = { completed[0].device_id };
    let c_seq = { completed[0].sequence_id };
    let c_status = { completed[0].status };
    let c_len = { completed[0].actual_length };

    assert_eq!(c_dev, test_device_id);
    assert_eq!(c_seq, 777);
    assert_eq!(c_status, 0);
    assert_eq!(c_len, 8);
    assert_eq!(&completed[0].data[..8], b"RESP-OK!");

    // Clean teardown
    let _ = shutdown_tx.send(true);
    let _ = tokio::time::timeout(Duration::from_millis(200), vhci_to_net_task).await;
    let _ = tokio::time::timeout(Duration::from_millis(200), net_to_vhci_task).await;
}

#[tokio::test]
async fn test_e2e_network_wire_forwarding_and_descriptor_resolution() {
    let vhci_client = Arc::new(VhciDriverClient::open().expect("VhciDriverClient open failed"));
    let test_device_id: u32 = 101;

    let (shutdown_tx, shutdown_rx_vhci) = watch::channel(false);
    let shutdown_rx_net = shutdown_tx.subscribe();
    let mut shutdown_rx_fwd = shutdown_tx.subscribe();

    // Data channels simulating the Guest pumps
    let (vhci_to_net_tx, mut vhci_to_net_rx) = mpsc::channel::<Vec<u8>>(32);
    let (net_to_vhci_tx, net_to_vhci_rx) = mpsc::channel::<Vec<u8>>(32);
    let (binary_outbound_tx, mut binary_outbound_rx) = mpsc::channel::<Vec<u8>>(32);

    let vhci_clone_1 = Arc::clone(&vhci_client);
    let vhci_to_net_task = tokio::spawn(async move {
        run_vhci_to_network_pump(vhci_clone_1, test_device_id, vhci_to_net_tx, shutdown_rx_vhci).await;
    });

    let vhci_clone_2 = Arc::clone(&vhci_client);
    let net_to_vhci_task = tokio::spawn(async move {
        run_network_to_vhci_pump(vhci_clone_2, net_to_vhci_rx, shutdown_rx_net).await;
    });

    // Network wire forwarder
    let binary_tx = binary_outbound_tx.clone();
    let net_fwd_task = tokio::spawn(async move {
        loop {
            tokio::select! {
                res = shutdown_rx_fwd.changed() => {
                    if res.is_err() || *shutdown_rx_fwd.borrow() { break; }
                }
                frame = vhci_to_net_rx.recv() => {
                    match frame {
                        Some(bytes) => {
                            if binary_tx.send(bytes).await.is_err() { break; }
                        }
                        None => break,
                    }
                }
            }
        }
    });

    // 1. Guest OS issues GET_DESCRIPTOR (18-byte Device Descriptor) into VHCI pending queue
    let mut submit_msg = AnyUsbUrbMessage::default();
    submit_msg.device_id = test_device_id;
    submit_msg.sequence_id = 1234;
    submit_msg.endpoint_address = 0x00; // Control EP0
    submit_msg.transfer_type = 0; // Control
    submit_msg.transfer_buffer_length = 18;
    // Setup packet for GET_DESCRIPTOR (Device): bmRequestType=0x80, bRequest=0x06, wValue=0x0100, wIndex=0, wLength=18
    submit_msg.setup_packet[0] = 0x80;
    submit_msg.setup_packet[1] = 0x06;
    submit_msg.setup_packet[2] = 0x00;
    submit_msg.setup_packet[3] = 0x01; // Descriptor type 1 = Device
    submit_msg.setup_packet[6] = 18;
    submit_msg.setup_packet[7] = 0;

    vhci_client.queue_simulated_urb(submit_msg);

    // 2. Receive the frame forwarded by net_fwd_task across the network wire
    let wire_frame = tokio::time::timeout(Duration::from_millis(500), binary_outbound_rx.recv())
        .await
        .expect("Timeout waiting for frame on network wire")
        .expect("Channel closed unexpectedly");

    assert!(wire_frame.len() >= TurboUrbHeader::SIZE);
    let parsed_hdr = TurboUrbHeader::deserialize_single_urb(&wire_frame[..TurboUrbHeader::SIZE])
        .expect("Valid TurboUrbHeader");
    assert_eq!(parsed_hdr.command, CMD_SUBMIT_URB);
    assert_eq!(parsed_hdr.sequence_id, 1234);

    // 3. Emulate Host USB response: synthesize 18-byte Device Descriptor
    let dev_desc = vec![
        18, 0x01, 0x00, 0x02, 0x00, 0x00, 0x00, 64,
        0x6D, 0x04, 0x77, 0xC0, 0x00, 0x01, 1, 2, 3, 1
    ];
    let mut resp_hdr = TurboUrbHeader::new(
        CMD_COMPLETE_URB,
        parsed_hdr.sequence_id,
        parsed_hdr.device_id,
        parsed_hdr.endpoint_address,
        parsed_hdr.transfer_type,
        18,
    );
    resp_hdr.status = 0;
    resp_hdr.actual_length = 18;

    let mut comp_frame = resp_hdr.serialize();
    comp_frame.extend_from_slice(&dev_desc);

    // Host sends back over network wire to net_to_vhci_tx
    net_to_vhci_tx.send(comp_frame).await.expect("Send completion frame");

    // 4. Verify VHCI completed the URB with Device Descriptor
    let mut completed = Vec::new();
    for _ in 0..20 {
        completed = vhci_client.get_completed_urbs();
        if !completed.is_empty() { break; }
        tokio::time::sleep(Duration::from_millis(15)).await;
    }

    assert_eq!(completed.len(), 1, "Expected 1 completed URB in VHCI client");
    let c_seq = { completed[0].sequence_id };
    let c_len = { completed[0].actual_length };
    assert_eq!(c_seq, 1234);
    assert_eq!(c_len, 18);
    assert_eq!(completed[0].data[0], 18, "bLength must be 18");
    assert_eq!(completed[0].data[1], 0x01, "bDescriptorType must be Device (1)");
    assert_eq!(completed[0].data[8], 0x6D, "Vendor ID low byte must match");
    assert_eq!(completed[0].data[9], 0x04, "Vendor ID high byte must match");

    // Clean teardown
    let _ = shutdown_tx.send(true);
    let _ = tokio::time::timeout(Duration::from_millis(200), vhci_to_net_task).await;
    let _ = tokio::time::timeout(Duration::from_millis(200), net_to_vhci_task).await;
    let _ = tokio::time::timeout(Duration::from_millis(200), net_fwd_task).await;
}

#[tokio::test]
async fn test_urb_64kb_boundary_enforcement() {
    let vhci = Arc::new(VhciDriverClient::open().expect("Open VHCI"));
    let filter = FilterDriverClient::open().expect("Open Filter");

    // 1. FilterDriverClient: exactly 64 KB is accepted
    let mut valid_comp = AnyUsbCompleteRequest::default();
    valid_comp.actual_length = ANYUSB_MAX_URB_BUFFER_SIZE as u32;
    assert!(filter.complete_urb(&valid_comp).is_ok());

    // 2. FilterDriverClient: 64 KB + 1 byte must be rejected with InvalidParameter
    let mut invalid_comp = AnyUsbCompleteRequest::default();
    invalid_comp.actual_length = (ANYUSB_MAX_URB_BUFFER_SIZE + 1) as u32;
    let err = filter.complete_urb(&invalid_comp).unwrap_err();
    match err {
        DriverClientError::InvalidParameter(msg) => {
            assert!(msg.contains("exceeds maximum single-URB buffer limit"));
        }
        other => panic!("Expected InvalidParameter error, got {:?}", other),
    }

    // 3. VhciDriverClient: boundary check
    assert!(vhci.complete_urb(&valid_comp).is_ok());
    let vhci_err = vhci.complete_urb(&invalid_comp).unwrap_err();
    match vhci_err {
        DriverClientError::InvalidParameter(msg) => {
            assert!(msg.contains("exceeds maximum single-URB buffer limit"));
        }
        other => panic!("Expected InvalidParameter error, got {:?}", other),
    }

    // 4. run_vhci_to_network_pump: oversized URB (> 64 KB) must be dropped
    let (shutdown_tx, shutdown_rx_vhci) = watch::channel(false);
    let (vhci_to_net_tx, mut vhci_to_net_rx) = mpsc::channel::<Vec<u8>>(16);

    let vhci_clone = Arc::clone(&vhci);
    let pump_task = tokio::spawn(async move {
        run_vhci_to_network_pump(vhci_clone, 55, vhci_to_net_tx, shutdown_rx_vhci).await;
    });

    let mut oversized_msg = AnyUsbUrbMessage::default();
    oversized_msg.sequence_id = 999;
    oversized_msg.transfer_buffer_length = (ANYUSB_MAX_URB_BUFFER_SIZE + 100) as u32;
    vhci.queue_simulated_urb(oversized_msg);

    // Queue valid URB right after
    let mut valid_msg = AnyUsbUrbMessage::default();
    valid_msg.sequence_id = 1000;
    valid_msg.transfer_buffer_length = 16;
    vhci.queue_simulated_urb(valid_msg);

    // Pump should drop oversized_msg (Seq 999) and output valid_msg (Seq 1000)
    let frame = tokio::time::timeout(Duration::from_millis(300), vhci_to_net_rx.recv())
        .await
        .expect("Timeout waiting for frame")
        .expect("Valid channel receive");

    let hdr = TurboUrbHeader::deserialize_single_urb(&frame[..TurboUrbHeader::SIZE]).unwrap();
    assert_eq!(
        hdr.sequence_id, 1000,
        "Oversized URB Seq 999 was not dropped!"
    );

    let _ = shutdown_tx.send(true);
    let _ = tokio::time::timeout(Duration::from_millis(100), pump_task).await;
}

#[tokio::test]
async fn test_adaptive_backoff_timing() {
    let vhci = Arc::new(VhciDriverClient::open().expect("Open VHCI"));
    let (shutdown_tx, shutdown_rx) = watch::channel(false);
    let (vhci_to_net_tx, mut vhci_to_net_rx) = mpsc::channel::<Vec<u8>>(16);

    let vhci_clone = Arc::clone(&vhci);
    let pump_task = tokio::spawn(async move {
        run_vhci_to_network_pump(vhci_clone, 1, vhci_to_net_tx, shutdown_rx).await;
    });

    // Let the pump idle for 20ms to allow backoff to increase up to MAX_BACKOFF (2ms)
    tokio::time::sleep(Duration::from_millis(20)).await;

    // Push work
    let start_time = std::time::Instant::now();
    let mut msg = AnyUsbUrbMessage::default();
    msg.sequence_id = 555;
    msg.transfer_buffer_length = 4;
    msg.data[..4].copy_from_slice(b"WAKE");
    vhci.queue_simulated_urb(msg);

    let frame = tokio::time::timeout(Duration::from_millis(50), vhci_to_net_rx.recv())
        .await
        .expect("Timeout waiting for frame after wake")
        .expect("Valid receive");

    let elapsed = start_time.elapsed();
    let hdr = TurboUrbHeader::deserialize_single_urb(&frame[..TurboUrbHeader::SIZE]).unwrap();
    assert_eq!(hdr.sequence_id, 555);
    // Even with MAX_BACKOFF (2ms), elapsed should be within well under 25ms
    assert!(
        elapsed < Duration::from_millis(25),
        "Adaptive backoff did not wake up promptly: elapsed = {:?}",
        elapsed
    );

    let _ = shutdown_tx.send(true);
    let _ = tokio::time::timeout(Duration::from_millis(100), pump_task).await;
}

#[test]
fn test_filter_stop_capture_drains_pending_and_queued_urbs() {
    let client = FilterDriverClient::open().expect("Open filter client");
    let dev1: u32 = 101;
    let dev2: u32 = 202;

    client.start_capture(dev1).expect("start dev1");
    client.start_capture(dev2).expect("start dev2");
    assert!(client.is_capturing(dev1));
    assert!(client.is_capturing(dev2));

    // Queue 2 URBs for dev1, 1 for dev2
    let mut msg1 = AnyUsbUrbMessage::default();
    msg1.device_id = dev1;
    msg1.sequence_id = 11;
    client.queue_simulated_urb(msg1);

    let mut msg2 = AnyUsbUrbMessage::default();
    msg2.device_id = dev1;
    msg2.sequence_id = 12;
    client.queue_simulated_urb(msg2);

    let mut msg3 = AnyUsbUrbMessage::default();
    msg3.device_id = dev2;
    msg3.sequence_id = 21;
    client.queue_simulated_urb(msg3);

    assert_eq!(client.pending_queue_len(), 3);

    // Stop capture on dev1: must drain only dev1's requests with STATUS_CANCELLED
    client.stop_capture(dev1).expect("stop capture dev1");
    assert!(!client.is_capturing(dev1));
    assert!(client.is_capturing(dev2));

    // Verify dev1 requests were drained and completed as cancelled
    assert_eq!(client.pending_queue_len(), 1);
    let completed = client.get_completed_urbs();
    assert_eq!(completed.len(), 2);
    const STATUS_CANCELLED: i32 = -1073741536; // 0xC0000120 as i32
    assert_eq!({ completed[0].device_id }, dev1);
    assert_eq!({ completed[0].sequence_id }, 11);
    assert_eq!({ completed[0].status }, STATUS_CANCELLED);
    assert_eq!({ completed[1].device_id }, dev1);
    assert_eq!({ completed[1].sequence_id }, 12);
    assert_eq!({ completed[1].status }, STATUS_CANCELLED);

    // Fetch remaining URB for dev2
    let fetched = client.fetch_urb().expect("fetch").expect("URB for dev2 present");
    assert_eq!({ fetched.device_id }, dev2);
    assert_eq!({ fetched.sequence_id }, 21);

    // Complete dev2 URB successfully
    let mut comp2 = AnyUsbCompleteRequest::default();
    comp2.device_id = dev2;
    comp2.sequence_id = 21;
    comp2.status = 0;
    client.complete_urb(&comp2).expect("complete comp2");

    // Stop capture on dev2
    client.stop_capture(dev2).expect("stop capture dev2");
    assert!(!client.is_capturing(dev2));
    assert_eq!(client.pending_queue_len(), 0);
}

#[test]
fn test_filter_stop_capture_all_devices_drains_everything() {
    let client = FilterDriverClient::open().expect("Open filter client");
    let dev1: u32 = 301;
    let dev2: u32 = 302;

    client.start_capture(dev1).expect("start dev1");
    client.start_capture(dev2).expect("start dev2");

    let mut msg1 = AnyUsbUrbMessage::default();
    msg1.device_id = dev1;
    msg1.sequence_id = 91;
    client.queue_simulated_urb(msg1);

    let mut msg2 = AnyUsbUrbMessage::default();
    msg2.device_id = dev2;
    msg2.sequence_id = 92;
    client.queue_simulated_urb(msg2);

    assert_eq!(client.pending_queue_len(), 2);

    // TargetDeviceId = 0: drains all devices
    client.stop_capture(0).expect("stop capture all");
    assert!(!client.is_capturing(dev1));
    assert!(!client.is_capturing(dev2));
    assert_eq!(client.pending_queue_len(), 0);

    let completed = client.get_completed_urbs();
    assert_eq!(completed.len(), 2);
    const STATUS_CANCELLED: i32 = -1073741536;
    assert_eq!({ completed[0].status }, STATUS_CANCELLED);
    assert_eq!({ completed[1].status }, STATUS_CANCELLED);
}

#[test]
fn test_filter_client_drop_drains_all_pending_requests() {
    let client = FilterDriverClient::open().expect("Open filter client");
    client.start_capture(401).unwrap();
    client.start_capture(402).unwrap();

    let mut msg1 = AnyUsbUrbMessage::default();
    msg1.device_id = 401;
    msg1.sequence_id = 101;
    client.queue_simulated_urb(msg1);

    let mut msg2 = AnyUsbUrbMessage::default();
    msg2.device_id = 402;
    msg2.sequence_id = 102;
    client.queue_simulated_urb(msg2);

    assert_eq!(client.pending_queue_len(), 2);

    // Explicit drop calls stop_capture(0) under Drop trait
    drop(client);
}
