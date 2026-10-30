//! USB Master Pro Core Library
//! Next-generation, high-performance, single-binary P2P USB virtualization engine.

pub mod adapters;
pub mod buffer_pool;
pub mod compression;
pub mod daemon;
pub mod driver_client;
pub mod enumerator;
pub mod hardware_db;
pub mod identity;
pub mod device_reset;
pub mod licensing;
pub mod loopback_test;
pub mod protocol;
pub mod security;
pub mod state;
pub mod urb_engine;
pub mod usb_descriptors;

pub use adapters::{
    AdapterError, AdapterRegistry, GpsGnssAdapter, IDeviceAdapter, IrisScannersAdapter,
    MantraMfs100Adapter, MorphoMso1300Adapter, SlapScannersAdapter, StartekFm220Adapter,
    UrbWireMessage,
};
pub use buffer_pool::BufferPool;
pub use compression::TurboCompressor;
pub use daemon::{
    derive_numeric_device_id, execute_session_disconnect, handle_share_device,
    run_filter_to_network_pump, run_network_to_filter_pump, run_network_to_vhci_pump,
    run_vhci_to_network_pump, spawn_session_pumps, start_daemon, ActiveSession, DaemonServer,
    DaemonState, RemoteMountedDevice, SessionPumpHandles,
};
pub use device_reset::{DeviceOperationalState, DeviceResetController, ResetMethod};
pub use driver_client::{
    check_driver_device_presence, check_driver_status, set_simulated_driver_status,
    AnyUsbAttachRequest, AnyUsbCompleteRequest, AnyUsbDetachRequest, AnyUsbFilterCaptureRequest,
    AnyUsbUrbMessage, DriverClientError, DriverOperationalStatus, FilterDriverClient,
    VhciDriverClient, ANYUSB_MAX_URB_BUFFER_SIZE,
};
pub use enumerator::{HardwareEnumerator, UsbDeviceInfo};
pub use hardware_db::{DeviceCategory, HardwareClassifier};
pub use identity::{
    validate_connection_id_format, verify_registration_proof, Ed25519Keypair, InstallationKeypair,
    NodeIdentity,
};
pub use licensing::{
    LicenseClaims, LicenseEngine, LicenseError, BHARTX_PRODUCTION_ROOT_LICENSE_PUBKEY,
    BHARTX_ROOT_LICENSE_PUBKEY,
};
#[cfg(any(test, feature = "test-utils"))]
pub use licensing::{
    BHARTX_ROOT_SEED, BHARTX_TEST_ROOT_SEED, STATIC_PRODUCTION_FIXTURE_HW_HASH,
    STATIC_PRODUCTION_FIXTURE_TOKEN,
};
pub use security::{
    KeyStorageProvider, PortableFileKeyStorage, SecurityError, WindowsDpapiKeyStorage,
};
pub use loopback_test::{SimulatedBiometricDevice, VirtualLoopbackPipeline};
pub use protocol::{
    classify_operation, deserialize_single_urb, validate_urb_buffer_boundary, OperationSafety,
    TransferType, TurboUrbHeader, CMD_ATTACH_DEVICE, CMD_COMPLETE_URB, CMD_DETACH_DEVICE,
    CMD_PING, CMD_PONG, CMD_RESET_DEVICE, CMD_SUBMIT_URB, CMD_UNLINK_URB, MAX_PAYLOAD_LENGTH,
};
pub use state::{can_transition, transitory_timeout, SessionState, SessionStateMachine, StateTransitionError};
pub use urb_engine::{
    BoundedUrbRing, PipeState, UrbCompletion, UrbEngine, UrbEngineError, UrbIsoPacket,
    UrbPayload, UrbPipe, UrbRequest, UsbSetupPacket, MAX_IN_FLIGHT_URBS, USBD_STATUS_CANCELED,
    USBD_STATUS_STALL_PID, USBD_STATUS_SUCCESS,
};
pub use usb_descriptors::{
    usbd_parse_configuration_descriptor_ex, usbd_parse_descriptors,
    UsbConfigurationDescriptor, UsbDeviceDescriptor, UsbEndpointDescriptor,
    UsbHidDescriptor, UsbInterfaceAssociationDescriptor, UsbInterfaceDescriptor,
    UsbParsedConfiguration, UsbParsedEndpoint, UsbParsedInterface,
};
