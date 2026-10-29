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
