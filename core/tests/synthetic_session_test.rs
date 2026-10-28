//! Synthetic Session End-to-End Integration Test
//! Validates P0 Core Foundation:
//! 1. Installation keypair generation + 9-digit ID validation (e.g. 354426246).
//! 2. Session state transition: Offline -> Initializing -> Ready -> Pairing -> Negotiating -> Connecting -> PreparingDevice -> Active.
//! 3. Exchanging 36-byte framed TurboUrbHeader packets with synthetic payload.
//! 4. Fault handling: simulated network drop transitioning Active -> Interrupted -> Recovering -> Closing -> Ready.
//! 5. Frame size violation test: sending packet > 16 MB is rejected.

use std::time::Duration;
use usb_master_core::identity::{validate_connection_id_format, InstallationKeypair};
use usb_master_core::protocol::{
    classify_operation, OperationSafety, TransferType, TurboUrbHeader, CMD_COMPLETE_URB,
    CMD_PING, CMD_PONG, CMD_RESET_DEVICE, CMD_SUBMIT_URB, MAX_PAYLOAD_LENGTH,
};
use usb_master_core::state::{can_transition, transitory_timeout, SessionState, SessionStateMachine};

/// Synthetic representation of a USB Master Pro client node in the network mesh
struct MockClientNode {
    pub name: String,
    pub id_raw: u32,
    pub id_formatted: String,
    pub keypair: InstallationKeypair,
    pub state_machine: SessionStateMachine,
}

impl MockClientNode {
    pub fn new(name: &str, id_raw: u32, id_formatted: &str) -> Self {
        assert!(validate_connection_id_format(id_formatted));
        assert!(validate_connection_id_format(&id_raw.to_string()));

        let keypair = InstallationKeypair::generate();
        let state_machine = SessionStateMachine::new();

        Self {
            name: name.to_string(),
            id_raw,
            id_formatted: id_formatted.to_string(),
            keypair,
            state_machine,
        }
    }
}

#[test]
fn test_synthetic_session_full_lifecycle_and_fault_recovery() {
    println!("\n=== MILESTONE P0: SYNTHETIC SESSION TEST EXECUTION ===");

    // ------------------------------------------------------------------------
    // Part 1: Installation keypair generation + 9-digit ID validation
    // ------------------------------------------------------------------------
    let test_id = "354426246";
    let formatted_test_id = "354 426 246";
    assert!(
        validate_connection_id_format(test_id),
        "Raw 9-digit ID 354426246 must pass validation"
    );
    assert!(
        validate_connection_id_format(formatted_test_id),
        "Formatted 9-digit ID '354 426 246' must pass validation"
    );

    // Initialize Node A (Host / Peripheral Sharer) and Node B (Client / Peripheral Consumer)
    let mut node_a = MockClientNode::new("NodeA-Host", 354426246, "354 426 246");
    let mut node_b = MockClientNode::new("NodeB-Client", 882104991, "882 104 991");

    assert_eq!(node_a.name, "NodeA-Host");
    assert_eq!(node_a.id_raw, 354426246);
    assert_eq!(node_a.id_formatted, "354 426 246");
    assert_eq!(node_b.name, "NodeB-Client");
    assert_eq!(node_b.id_raw, 882104991);
    assert_eq!(node_b.id_formatted, "882 104 991");

    assert_eq!(node_a.keypair.public_key().len(), 32);
    assert_eq!(node_b.keypair.public_key().len(), 32);
    assert_ne!(node_a.keypair.public_key(), node_b.keypair.public_key());

    // Mutual challenge-response demonstration
    let challenge_nonce = b"CHALLENGE_NONCE_P0_SESSION_INIT_90412";
    let signature_a = node_a.keypair.sign(challenge_nonce);
    assert!(
        InstallationKeypair::verify_with_public_key(
            node_a.keypair.public_key(),
            challenge_nonce,
            &signature_a
        ),
        "Node B must verify Node A's asymmetric cryptographic signature"
    );

    // ------------------------------------------------------------------------
    // Part 2: Session state transition sequence
    // Offline -> Initializing -> Ready -> Pairing -> Negotiating -> Connecting -> PreparingDevice -> Active
    // ------------------------------------------------------------------------
    assert_eq!(node_a.state_machine.current_state(), SessionState::Offline);
    assert_eq!(node_b.state_machine.current_state(), SessionState::Offline);

    let forward_transitions = [
        (SessionState::Initializing, "Daemon service bootstrapping"),
        (SessionState::Ready, "IPC and hardware enumerator online"),
        (SessionState::Pairing, "Remote node connection request initiated"),
        (SessionState::Negotiating, "PIN validated; exchanging cryptographic capabilities"),
        (SessionState::Connecting, "Establishing P2P WebRTC data tunnel"),
        (SessionState::PreparingDevice, "Attaching upper filter driver to USB peripheral"),
        (SessionState::Active, "Turbo-URB streaming operational"),
    ];

    for (target_state, reason) in &forward_transitions {
        // Assert can_transition validator before executing transition
        assert!(
            can_transition(node_a.state_machine.current_state(), *target_state),
            "Deterministic validator must permit {:?} -> {:?}",
            node_a.state_machine.current_state(),
            target_state
        );

        node_a
            .state_machine
            .transition_to(*target_state, Some(reason.to_string()))
            .unwrap_or_else(|e| panic!("Node A transition to {:?} failed: {}", target_state, e));

        node_b
            .state_machine
            .transition_to(*target_state, Some(reason.to_string()))
            .unwrap_or_else(|e| panic!("Node B transition to {:?} failed: {}", target_state, e));

        assert_eq!(node_a.state_machine.current_state(), *target_state);
        assert_eq!(node_b.state_machine.current_state(), *target_state);
    }

    assert_eq!(node_a.state_machine.current_state(), SessionState::Active);
    assert_eq!(node_b.state_machine.current_state(), SessionState::Active);

    // ------------------------------------------------------------------------
    // Part 3: Exchanging 36-byte framed TurboUrbHeader packets with synthetic payload
    // ------------------------------------------------------------------------
    let sequence_id: u64 = 554201;
    let device_id: u32 = 1;
    let endpoint_in: u8 = 0x81; // Bulk IN endpoint (biometric sensor read)
    let payload_size: u32 = 512;

    // Operation safety check
    let op_safety = classify_operation(CMD_SUBMIT_URB, TransferType::Bulk as u8, endpoint_in);
    assert_eq!(
        op_safety,
        OperationSafety::IdempotentSafeToRetry,
        "Bulk IN read must be classified as IdempotentSafeToRetry"
    );

    // Node B submits a URB request packet
    let submit_hdr = TurboUrbHeader::new(
        CMD_SUBMIT_URB,
        sequence_id,
        device_id,
        endpoint_in,
        TransferType::Bulk,
        payload_size,
    );
    let submit_wire = submit_hdr.serialize();
    assert_eq!(
        submit_wire.len(),
        TurboUrbHeader::SIZE,
        "TurboUrbHeader must strictly occupy 36 bytes on the wire"
    );

    // Node A receives and parses the request
    let received_submit = TurboUrbHeader::deserialize(&submit_wire)
        .expect("Node A failed to deserialize TurboUrbHeader request");
    assert_eq!(received_submit.command, CMD_SUBMIT_URB);
    assert_eq!(received_submit.sequence_id, sequence_id);
    assert_eq!(received_submit.buffer_length, payload_size);

    // Node A synthesizes biometric raw image frame (512 bytes)
    let synthetic_payload: Vec<u8> = (0..payload_size).map(|i| (i % 251) as u8).collect();

    // Node A constructs completion header
    let mut complete_hdr = TurboUrbHeader::new(
        CMD_COMPLETE_URB,
        sequence_id,
        device_id,
        endpoint_in,
        TransferType::Bulk,
        payload_size,
    );
    complete_hdr.status = 0; // USBD_STATUS_SUCCESS
    complete_hdr.actual_length = payload_size;

    // Node A frames [36-byte header] + [512-byte payload]
    let mut complete_wire_packet = complete_hdr.serialize();
    assert_eq!(complete_wire_packet.len(), 36);
    complete_wire_packet.extend_from_slice(&synthetic_payload);
    assert_eq!(complete_wire_packet.len(), 36 + (payload_size as usize));

    // Node B receives and unpacks the framed packet
    let received_complete_hdr = TurboUrbHeader::deserialize(&complete_wire_packet[0..36])
        .expect("Node B failed to deserialize 36-byte TurboUrbHeader response");
    assert_eq!(received_complete_hdr.command, CMD_COMPLETE_URB);
    assert_eq!(received_complete_hdr.sequence_id, sequence_id);
    assert_eq!(received_complete_hdr.status, 0);
    assert_eq!(received_complete_hdr.actual_length, payload_size);

    let received_payload = &complete_wire_packet[36..36 + (received_complete_hdr.actual_length as usize)];
    assert_eq!(
        received_payload,
        synthetic_payload.as_slice(),
        "Payload received by Node B must match synthetic biometric frame byte-for-byte"
    );

    // Verify non-idempotent operation classification
    let op_safety_write = classify_operation(CMD_SUBMIT_URB, TransferType::Bulk as u8, 0x02);
    assert_eq!(
        op_safety_write,
        OperationSafety::NonIdempotentUncertain,
        "Bulk OUT writes must be classified as NonIdempotentUncertain"
    );

    // ------------------------------------------------------------------------
    // Part 4: Fault handling: simulated network drop
    // Active -> Interrupted -> Recovering -> Closing -> Ready
    // ------------------------------------------------------------------------
    // Active -> Interrupted (Heartbeat loss simulated)
    node_a
        .state_machine
        .transition_to(SessionState::Interrupted, Some("Keepalive ping lost".into()))
        .expect("Active -> Interrupted must succeed");
    assert_eq!(node_a.state_machine.current_state(), SessionState::Interrupted);

    // Interrupted -> Recovering (ICE reconnect attempt)
    node_a
        .state_machine
        .transition_to(SessionState::Recovering, Some("Initiating ICE restart".into()))
        .expect("Interrupted -> Recovering must succeed");
    assert_eq!(node_a.state_machine.current_state(), SessionState::Recovering);
    assert_eq!(
        node_a.state_machine.timeout_duration(),
        Some(Duration::from_secs(10)),
        "Recovering transitory state must have a 10s timeout ceiling"
    );

    // Recovering -> Closing (Recovery failed / timed out)
    node_a
        .state_machine
        .transition_to(SessionState::Closing, Some("Reconnection timeout reached".into()))
        .expect("Recovering -> Closing must succeed");
    assert_eq!(node_a.state_machine.current_state(), SessionState::Closing);

    // Closing -> Ready (Teardown complete, ready for new connections)
    node_a
        .state_machine
        .transition_to(SessionState::Ready, Some("Teardown clean; listener reset".into()))
        .expect("Closing -> Ready must succeed");
    assert_eq!(node_a.state_machine.current_state(), SessionState::Ready);

    // ------------------------------------------------------------------------
    // Part 5: Frame size violation test: packet > 16 MB is rejected
    // ------------------------------------------------------------------------
    let max_allowed = MAX_PAYLOAD_LENGTH;
    assert_eq!(max_allowed, 16 * 1024 * 1024);

    // Exactly 16 MB is the maximum permitted ceiling
    let mut exact_hdr = TurboUrbHeader::new(CMD_SUBMIT_URB, 999, 1, 0x81, TransferType::Bulk, max_allowed);
    exact_hdr.actual_length = max_allowed;
    let exact_bytes = exact_hdr.serialize();
    assert!(
        TurboUrbHeader::deserialize(&exact_bytes).is_ok(),
        "16 MB exact packet length must be accepted"
    );

    // Frame with buffer_length > 16 MB must be rejected
    let oversize_buf_hdr = TurboUrbHeader::new(
        CMD_SUBMIT_URB,
        1001,
        1,
        0x81,
        TransferType::Bulk,
        max_allowed + 1,
    );
    let oversize_buf_bytes = oversize_buf_hdr.serialize();
    let err_buf = TurboUrbHeader::deserialize(&oversize_buf_bytes).unwrap_err();
    assert_eq!(
        err_buf,
        "Frame length exceeds maximum allowed ceiling (16 MB)",
        "Over-limit buffer_length must be rejected with exact error message"
    );

    // Frame with actual_length > 16 MB must also be rejected
    let mut oversize_act_hdr = TurboUrbHeader::new(
        CMD_COMPLETE_URB,
        1002,
        1,
        0x81,
        TransferType::Bulk,
        1024,
    );
    oversize_act_hdr.actual_length = max_allowed + 1024;
    let oversize_act_bytes = oversize_act_hdr.serialize();
    let err_act = TurboUrbHeader::deserialize(&oversize_act_bytes).unwrap_err();
    assert_eq!(
        err_act,
        "Frame length exceeds maximum allowed ceiling (16 MB)",
        "Over-limit actual_length must be rejected with exact error message"
    );

    println!("=== ALL 5 P0 REQUIREMENTS VERIFIED WITH 100% PASSING TELEMETRY ===\n");
}

#[test]
fn test_synthetic_transitory_timeouts_and_rejections() {
    // Verify transitory timeouts match Master Blueprint Section 5.4 exactly
    assert_eq!(transitory_timeout(SessionState::Pairing), Some(Duration::from_secs(30)));
    assert_eq!(transitory_timeout(SessionState::Negotiating), Some(Duration::from_secs(15)));
    assert_eq!(transitory_timeout(SessionState::Connecting), Some(Duration::from_secs(20)));
    assert_eq!(transitory_timeout(SessionState::Recovering), Some(Duration::from_secs(10)));
    assert_eq!(transitory_timeout(SessionState::Active), None);
    assert_eq!(transitory_timeout(SessionState::Offline), None);

    // Illegal jump rejection
    let mut sm = SessionStateMachine::new();
    assert_eq!(sm.current_state(), SessionState::Offline);
    assert!(!sm.can_transition(SessionState::Active));
    assert!(sm.transition_to(SessionState::Active, None).is_err());

    // Ping / Pong Idempotency
    assert_eq!(classify_operation(CMD_PING, 0, 0), OperationSafety::IdempotentSafeToRetry);
    assert_eq!(classify_operation(CMD_PONG, 0, 0), OperationSafety::IdempotentSafeToRetry);
    assert_eq!(classify_operation(CMD_RESET_DEVICE, 0, 0), OperationSafety::NonIdempotentUncertain);
}
