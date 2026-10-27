//! USB Master Pro Core Library
//! Next-generation, high-performance, single-binary P2P USB virtualization engine.

pub mod buffer_pool;
pub mod compression;
pub mod daemon;
pub mod enumerator;
pub mod hardware_db;
pub mod identity;
pub mod device_reset;
pub mod loopback_test;
pub mod protocol;
pub mod urb_engine;
pub mod usb_descriptors;

pub use buffer_pool::BufferPool;
pub use compression::TurboCompressor;
pub use daemon::DaemonServer;
pub use device_reset::{DeviceOperationalState, DeviceResetController, ResetMethod};
pub use enumerator::{HardwareEnumerator, UsbDeviceInfo};
pub use hardware_db::{DeviceCategory, HardwareClassifier};
pub use identity::NodeIdentity;
pub use loopback_test::{SimulatedBiometricDevice, VirtualLoopbackPipeline};
pub use protocol::{TurboUrbHeader, TransferType, CMD_SUBMIT_URB, CMD_COMPLETE_URB};
pub use urb_engine::{
    PipeState, UrbCompletion, UrbEngine, UrbIsoPacket, UrbPayload, UrbPipe, UrbRequest,
    UsbSetupPacket, USBD_STATUS_CANCELED, USBD_STATUS_STALL_PID, USBD_STATUS_SUCCESS,
};
pub use usb_descriptors::{
    usbd_parse_configuration_descriptor_ex, usbd_parse_descriptors,
    UsbConfigurationDescriptor, UsbDeviceDescriptor, UsbEndpointDescriptor,
    UsbHidDescriptor, UsbInterfaceAssociationDescriptor, UsbInterfaceDescriptor,
    UsbParsedConfiguration, UsbParsedEndpoint, UsbParsedInterface,
};
