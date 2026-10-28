//! Hardware Device Adapters Framework for USB Master Pro
//! Certified OEM hardware acceleration and URB optimization pipelines.

pub mod gps_gnss;
pub mod iris_scanners;
pub mod mantra_mfs100;
pub mod morpho_mso1300;
pub mod slap_scanners;

pub use gps_gnss::GpsGnssAdapter;
pub use iris_scanners::IrisScannersAdapter;
pub use mantra_mfs100::MantraMfs100Adapter;
pub use morpho_mso1300::{MorphoMso1300Adapter, StartekFm220Adapter};
pub use slap_scanners::SlapScannersAdapter;

use crate::hardware_db::DeviceCategory;
use crate::protocol::{TransferType, TurboUrbHeader};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

/// In-flight or wire URB message containing fixed header and variable payload
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UrbWireMessage {
    pub header: TurboUrbHeader,
    pub payload: Vec<u8>,
}

impl UrbWireMessage {
    pub fn new(header: TurboUrbHeader, payload: Vec<u8>) -> Self {
        Self { header, payload }
    }
}

/// Errors occurring during adapter-specific URB transformations
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum AdapterError {
    #[error("Device not supported: VID=0x{0:04X}, PID=0x{1:04X}")]
    DeviceNotSupported(u16, u16),

    #[error("Packet exceeds maximum allowed size for adapter (max: {max_size}, got: {actual_size})")]
    PacketTooLarge { max_size: usize, actual_size: usize },

    #[error("Transfer type not supported for adapter: {0:?}")]
    UnsupportedTransferType(TransferType),

    #[error("Optimization failed: {0}")]
    OptimizationError(String),
}

/// Unified trait implemented by certified hardware device adapters
pub trait IDeviceAdapter: Send + Sync {
    /// Friendly marketing and engineering name of the adapter
    fn name(&self) -> &str;

    /// Device classification category
    fn category(&self) -> DeviceCategory;

    /// Returns true if this adapter handles the specified USB Vendor & Product ID
    fn matches(&self, vid: u16, pid: u16) -> bool;

    /// Applies real-time hardware-specific latency and throughput optimizations to the URB
    fn optimize_urb(&self, urb: &mut UrbWireMessage) -> Result<(), AdapterError>;

    /// Hardware flags required for this device category (e.g., FLAG_RD_SERVICE_OPTIMIZED)
    fn required_flags(&self) -> u32;

    /// Maximum transfer packet / buffer size supported by this adapter
    fn max_packet_size(&self) -> usize;

    /// Maximum roundtrip SLA target in milliseconds under USB-over-IP virtualization
    fn target_sla_ms(&self) -> u32;
}

/// Central registry of certified OEM device adapters
pub struct AdapterRegistry {
    adapters: Vec<Arc<dyn IDeviceAdapter>>,
}

impl AdapterRegistry {
    /// Creates a new AdapterRegistry pre-populated with all certified adapters
    pub fn new() -> Self {
        Self {
            adapters: vec![
                Arc::new(MantraMfs100Adapter::new()),
                Arc::new(MorphoMso1300Adapter::new()),
                Arc::new(SlapScannersAdapter::new()),
                Arc::new(IrisScannersAdapter::new()),
                Arc::new(GpsGnssAdapter::new()),
            ],
        }
    }

    /// Registers a new or custom hardware adapter
    pub fn register(&mut self, adapter: Arc<dyn IDeviceAdapter>) {
        self.adapters.push(adapter);
    }

    /// Finds a matching certified adapter for the given USB VID and PID on this instance
    pub fn lookup(&self, vid: u16, pid: u16) -> Option<Arc<dyn IDeviceAdapter>> {
        self.adapters.iter().find(|a| a.matches(vid, pid)).cloned()
    }

    /// Returns a list of all registered adapters on this instance
    pub fn get_all(&self) -> Vec<Arc<dyn IDeviceAdapter>> {
        self.adapters.clone()
    }

    /// Global singleton instance of the certified AdapterRegistry
    pub fn global() -> &'static AdapterRegistry {
        static INSTANCE: std::sync::OnceLock<AdapterRegistry> = std::sync::OnceLock::new();
        INSTANCE.get_or_init(Self::new)
    }

    /// Look up an adapter by VID and PID using the global registry
    pub fn find_adapter(vid: u16, pid: u16) -> Option<Arc<dyn IDeviceAdapter>> {
        Self::global().lookup(vid, pid)
    }

    /// Returns all registered certified adapters from the global registry
    pub fn all_adapters() -> Vec<Arc<dyn IDeviceAdapter>> {
        Self::global().get_all()
    }
}

impl Default for AdapterRegistry {
    fn default() -> Self {
        Self::new()
    }
}
