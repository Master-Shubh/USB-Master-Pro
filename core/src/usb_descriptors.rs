//! USB 2.0 / USB 3.x Full Descriptor Parser & USBD Specification Utilities
//! Implements strict parsing, validation, and reconstruction equivalent to
//! Windows USBD (`USBD_ParseConfigurationDescriptorEx`) and Linux USB core descriptors.

use byteorder::{LittleEndian, ReadBytesExt, WriteBytesExt};
use serde::{Deserialize, Serialize};
use std::io::Cursor;

// -----------------------------------------------------------------------------
// Standard USB Descriptor Types (USB 2.0 Spec Table 9-5 & USB 3.2 Spec Table 9-16)
// -----------------------------------------------------------------------------
pub const DESC_TYPE_DEVICE: u8 = 0x01;
pub const DESC_TYPE_CONFIGURATION: u8 = 0x02;
pub const DESC_TYPE_STRING: u8 = 0x03;
pub const DESC_TYPE_INTERFACE: u8 = 0x04;
pub const DESC_TYPE_ENDPOINT: u8 = 0x05;
pub const DESC_TYPE_DEVICE_QUALIFIER: u8 = 0x06;
pub const DESC_TYPE_OTHER_SPEED_CONFIGURATION: u8 = 0x07;
pub const DESC_TYPE_INTERFACE_POWER: u8 = 0x08;
pub const DESC_TYPE_OTG: u8 = 0x09;
pub const DESC_TYPE_DEBUG: u8 = 0x0A;
pub const DESC_TYPE_INTERFACE_ASSOCIATION: u8 = 0x0B; // IAD
pub const DESC_TYPE_BOS: u8 = 0x0F;
pub const DESC_TYPE_DEVICE_CAPABILITY: u8 = 0x10;
pub const DESC_TYPE_HID: u8 = 0x21;
pub const DESC_TYPE_HID_REPORT: u8 = 0x22;
pub const DESC_TYPE_PHYSICAL: u8 = 0x23;
pub const DESC_TYPE_CS_INTERFACE: u8 = 0x24;
pub const DESC_TYPE_CS_ENDPOINT: u8 = 0x25;
pub const DESC_TYPE_SS_ENDPOINT_COMPANION: u8 = 0x30;

// Standard lengths
pub const LEN_DEVICE_DESCRIPTOR: u8 = 18;
pub const LEN_CONFIGURATION_DESCRIPTOR: u8 = 9;
pub const LEN_INTERFACE_DESCRIPTOR: u8 = 9;
pub const LEN_ENDPOINT_DESCRIPTOR: u8 = 7;
pub const LEN_SS_ENDPOINT_COMPANION: u8 = 6;
pub const LEN_IAD_DESCRIPTOR: u8 = 8;
pub const LEN_BOS_DESCRIPTOR: u8 = 5;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UsbDescriptorError {
    BufferTooShort { expected: usize, actual: usize },
    InvalidLength { descriptor_type: u8, length: u8 },
    InvalidType { expected: u8, actual: u8 },
    MalformedDescriptor(String),
}

impl std::fmt::Display for UsbDescriptorError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::BufferTooShort { expected, actual } => {
                write!(f, "Buffer too short: expected at least {} bytes, found {}", expected, actual)
            }
            Self::InvalidLength { descriptor_type, length } => {
                write!(f, "Invalid descriptor length {} for type 0x{:02X}", length, descriptor_type)
            }
            Self::InvalidType { expected, actual } => {
                write!(f, "Unexpected descriptor type 0x{:02X}, expected 0x{:02X}", actual, expected)
            }
            Self::MalformedDescriptor(msg) => write!(f, "Malformed descriptor: {}", msg),
        }
    }
}

impl std::error::Error for UsbDescriptorError {}

// -----------------------------------------------------------------------------
// Endpoint Direction & Transfer Type
// -----------------------------------------------------------------------------
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EndpointDirection {
    Out = 0,
    In = 1,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EndpointTransferType {
    Control = 0,
    Isochronous = 1,
    Bulk = 2,
    Interrupt = 3,
}

// -----------------------------------------------------------------------------
// 1. USB Device Descriptor (USB 2.0 Spec §9.6.1)
// -----------------------------------------------------------------------------
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UsbDeviceDescriptor {
    pub b_length: u8,
    pub b_descriptor_type: u8,
    pub bcd_usb: u16,
    pub b_device_class: u8,
    pub b_device_sub_class: u8,
    pub b_device_protocol: u8,
    pub b_max_packet_size0: u8,
    pub id_vendor: u16,
    pub id_product: u16,
    pub bcd_device: u16,
    pub i_manufacturer: u8,
    pub i_product: u8,
    pub i_serial_number: u8,
    pub b_num_configurations: u8,
}

impl UsbDeviceDescriptor {
    pub fn parse(bytes: &[u8]) -> Result<Self, UsbDescriptorError> {
        if bytes.len() < LEN_DEVICE_DESCRIPTOR as usize {
            return Err(UsbDescriptorError::BufferTooShort {
                expected: LEN_DEVICE_DESCRIPTOR as usize,
                actual: bytes.len(),
            });
        }
        let b_length = bytes[0];
        let b_descriptor_type = bytes[1];
        if b_length < LEN_DEVICE_DESCRIPTOR {
            return Err(UsbDescriptorError::InvalidLength {
                descriptor_type: b_descriptor_type,
                length: b_length,
            });
        }
        if b_descriptor_type != DESC_TYPE_DEVICE {
            return Err(UsbDescriptorError::InvalidType {
                expected: DESC_TYPE_DEVICE,
                actual: b_descriptor_type,
            });
        }

        let mut rdr = Cursor::new(bytes);
        let _ = rdr.read_u8().map_err(|e| UsbDescriptorError::MalformedDescriptor(e.to_string()))?;
        let _ = rdr.read_u8().map_err(|e| UsbDescriptorError::MalformedDescriptor(e.to_string()))?;
        let bcd_usb = rdr.read_u16::<LittleEndian>().map_err(|e| UsbDescriptorError::MalformedDescriptor(e.to_string()))?;
        let b_device_class = rdr.read_u8().map_err(|e| UsbDescriptorError::MalformedDescriptor(e.to_string()))?;
        let b_device_sub_class = rdr.read_u8().map_err(|e| UsbDescriptorError::MalformedDescriptor(e.to_string()))?;
        let b_device_protocol = rdr.read_u8().map_err(|e| UsbDescriptorError::MalformedDescriptor(e.to_string()))?;
        let b_max_packet_size0 = rdr.read_u8().map_err(|e| UsbDescriptorError::MalformedDescriptor(e.to_string()))?;
        let id_vendor = rdr.read_u16::<LittleEndian>().map_err(|e| UsbDescriptorError::MalformedDescriptor(e.to_string()))?;
        let id_product = rdr.read_u16::<LittleEndian>().map_err(|e| UsbDescriptorError::MalformedDescriptor(e.to_string()))?;
        let bcd_device = rdr.read_u16::<LittleEndian>().map_err(|e| UsbDescriptorError::MalformedDescriptor(e.to_string()))?;
        let i_manufacturer = rdr.read_u8().map_err(|e| UsbDescriptorError::MalformedDescriptor(e.to_string()))?;
        let i_product = rdr.read_u8().map_err(|e| UsbDescriptorError::MalformedDescriptor(e.to_string()))?;
        let i_serial_number = rdr.read_u8().map_err(|e| UsbDescriptorError::MalformedDescriptor(e.to_string()))?;
        let b_num_configurations = rdr.read_u8().map_err(|e| UsbDescriptorError::MalformedDescriptor(e.to_string()))?;

        Ok(Self {
            b_length,
            b_descriptor_type,
            bcd_usb,
            b_device_class,
            b_device_sub_class,
            b_device_protocol,
            b_max_packet_size0,
            id_vendor,
            id_product,
            bcd_device,
            i_manufacturer,
            i_product,
            i_serial_number,
            b_num_configurations,
        })
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        let mut buf = Vec::with_capacity(LEN_DEVICE_DESCRIPTOR as usize);
        buf.write_u8(self.b_length).unwrap();
        buf.write_u8(self.b_descriptor_type).unwrap();
        buf.write_u16::<LittleEndian>(self.bcd_usb).unwrap();
        buf.write_u8(self.b_device_class).unwrap();
        buf.write_u8(self.b_device_sub_class).unwrap();
        buf.write_u8(self.b_device_protocol).unwrap();
        buf.write_u8(self.b_max_packet_size0).unwrap();
        buf.write_u16::<LittleEndian>(self.id_vendor).unwrap();
        buf.write_u16::<LittleEndian>(self.id_product).unwrap();
        buf.write_u16::<LittleEndian>(self.bcd_device).unwrap();
        buf.write_u8(self.i_manufacturer).unwrap();
        buf.write_u8(self.i_product).unwrap();
        buf.write_u8(self.i_serial_number).unwrap();
        buf.write_u8(self.b_num_configurations).unwrap();
        buf
    }
}

// -----------------------------------------------------------------------------
// 2. USB Configuration Descriptor (USB 2.0 Spec §9.6.3)
// -----------------------------------------------------------------------------
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UsbConfigurationDescriptor {
    pub b_length: u8,
    pub b_descriptor_type: u8,
    pub w_total_length: u16,
    pub b_num_interfaces: u8,
    pub b_configuration_value: u8,
    pub i_configuration: u8,
    pub bm_attributes: u8,
    pub b_max_power: u8,
}

impl UsbConfigurationDescriptor {
    pub fn parse(bytes: &[u8]) -> Result<Self, UsbDescriptorError> {
        if bytes.len() < LEN_CONFIGURATION_DESCRIPTOR as usize {
            return Err(UsbDescriptorError::BufferTooShort {
                expected: LEN_CONFIGURATION_DESCRIPTOR as usize,
                actual: bytes.len(),
            });
        }
        let b_length = bytes[0];
        let b_descriptor_type = bytes[1];
        if b_length < LEN_CONFIGURATION_DESCRIPTOR {
            return Err(UsbDescriptorError::InvalidLength {
                descriptor_type: b_descriptor_type,
                length: b_length,
            });
        }
        if b_descriptor_type != DESC_TYPE_CONFIGURATION {
            return Err(UsbDescriptorError::InvalidType {
                expected: DESC_TYPE_CONFIGURATION,
                actual: b_descriptor_type,
            });
        }

        let mut rdr = Cursor::new(bytes);
        let _ = rdr.read_u8().map_err(|e| UsbDescriptorError::MalformedDescriptor(e.to_string()))?;
        let _ = rdr.read_u8().map_err(|e| UsbDescriptorError::MalformedDescriptor(e.to_string()))?;
        let w_total_length = rdr.read_u16::<LittleEndian>().map_err(|e| UsbDescriptorError::MalformedDescriptor(e.to_string()))?;
        let b_num_interfaces = rdr.read_u8().map_err(|e| UsbDescriptorError::MalformedDescriptor(e.to_string()))?;
        let b_configuration_value = rdr.read_u8().map_err(|e| UsbDescriptorError::MalformedDescriptor(e.to_string()))?;
        let i_configuration = rdr.read_u8().map_err(|e| UsbDescriptorError::MalformedDescriptor(e.to_string()))?;
        let bm_attributes = rdr.read_u8().map_err(|e| UsbDescriptorError::MalformedDescriptor(e.to_string()))?;
        let b_max_power = rdr.read_u8().map_err(|e| UsbDescriptorError::MalformedDescriptor(e.to_string()))?;

        Ok(Self {
            b_length,
            b_descriptor_type,
            w_total_length,
            b_num_interfaces,
            b_configuration_value,
            i_configuration,
            bm_attributes,
            b_max_power,
        })
    }

    pub fn is_self_powered(&self) -> bool {
        (self.bm_attributes & 0x40) != 0
    }

    pub fn supports_remote_wakeup(&self) -> bool {
        (self.bm_attributes & 0x20) != 0
    }

    pub fn max_power_ma(&self) -> u32 {
        self.b_max_power as u32 * 2
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        let mut buf = Vec::with_capacity(LEN_CONFIGURATION_DESCRIPTOR as usize);
        buf.write_u8(self.b_length).unwrap();
        buf.write_u8(self.b_descriptor_type).unwrap();
        buf.write_u16::<LittleEndian>(self.w_total_length).unwrap();
        buf.write_u8(self.b_num_interfaces).unwrap();
        buf.write_u8(self.b_configuration_value).unwrap();
        buf.write_u8(self.i_configuration).unwrap();
        buf.write_u8(self.bm_attributes).unwrap();
        buf.write_u8(self.b_max_power).unwrap();
        buf
    }
}

// -----------------------------------------------------------------------------
// 3. USB Interface Descriptor (USB 2.0 Spec §9.6.5)
// -----------------------------------------------------------------------------
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UsbInterfaceDescriptor {
    pub b_length: u8,
    pub b_descriptor_type: u8,
    pub b_interface_number: u8,
    pub b_alternate_setting: u8,
    pub b_num_endpoints: u8,
    pub b_interface_class: u8,
    pub b_interface_sub_class: u8,
    pub b_interface_protocol: u8,
    pub i_interface: u8,
}

impl UsbInterfaceDescriptor {
    pub fn parse(bytes: &[u8]) -> Result<Self, UsbDescriptorError> {
        if bytes.len() < LEN_INTERFACE_DESCRIPTOR as usize {
            return Err(UsbDescriptorError::BufferTooShort {
                expected: LEN_INTERFACE_DESCRIPTOR as usize,
                actual: bytes.len(),
            });
        }
        let b_length = bytes[0];
        let b_descriptor_type = bytes[1];
        if b_length < LEN_INTERFACE_DESCRIPTOR {
            return Err(UsbDescriptorError::InvalidLength {
                descriptor_type: b_descriptor_type,
                length: b_length,
            });
        }
        if b_descriptor_type != DESC_TYPE_INTERFACE {
            return Err(UsbDescriptorError::InvalidType {
                expected: DESC_TYPE_INTERFACE,
                actual: b_descriptor_type,
            });
        }

        let mut rdr = Cursor::new(bytes);
        let _ = rdr.read_u8().map_err(|e| UsbDescriptorError::MalformedDescriptor(e.to_string()))?;
        let _ = rdr.read_u8().map_err(|e| UsbDescriptorError::MalformedDescriptor(e.to_string()))?;
        let b_interface_number = rdr.read_u8().map_err(|e| UsbDescriptorError::MalformedDescriptor(e.to_string()))?;
        let b_alternate_setting = rdr.read_u8().map_err(|e| UsbDescriptorError::MalformedDescriptor(e.to_string()))?;
        let b_num_endpoints = rdr.read_u8().map_err(|e| UsbDescriptorError::MalformedDescriptor(e.to_string()))?;
        let b_interface_class = rdr.read_u8().map_err(|e| UsbDescriptorError::MalformedDescriptor(e.to_string()))?;
        let b_interface_sub_class = rdr.read_u8().map_err(|e| UsbDescriptorError::MalformedDescriptor(e.to_string()))?;
        let b_interface_protocol = rdr.read_u8().map_err(|e| UsbDescriptorError::MalformedDescriptor(e.to_string()))?;
        let i_interface = rdr.read_u8().map_err(|e| UsbDescriptorError::MalformedDescriptor(e.to_string()))?;

        Ok(Self {
            b_length,
            b_descriptor_type,
            b_interface_number,
            b_alternate_setting,
            b_num_endpoints,
            b_interface_class,
            b_interface_sub_class,
            b_interface_protocol,
            i_interface,
        })
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        let mut buf = Vec::with_capacity(LEN_INTERFACE_DESCRIPTOR as usize);
        buf.write_u8(self.b_length).unwrap();
        buf.write_u8(self.b_descriptor_type).unwrap();
        buf.write_u8(self.b_interface_number).unwrap();
        buf.write_u8(self.b_alternate_setting).unwrap();
        buf.write_u8(self.b_num_endpoints).unwrap();
        buf.write_u8(self.b_interface_class).unwrap();
        buf.write_u8(self.b_interface_sub_class).unwrap();
        buf.write_u8(self.b_interface_protocol).unwrap();
        buf.write_u8(self.i_interface).unwrap();
        buf
    }
}

// -----------------------------------------------------------------------------
// 4. USB Endpoint Descriptor (USB 2.0 Spec §9.6.6)
// -----------------------------------------------------------------------------
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UsbEndpointDescriptor {
    pub b_length: u8,
    pub b_descriptor_type: u8,
    pub b_endpoint_address: u8,
    pub bm_attributes: u8,
    pub w_max_packet_size: u16,
    pub b_interval: u8,
}

impl UsbEndpointDescriptor {
    pub fn parse(bytes: &[u8]) -> Result<Self, UsbDescriptorError> {
        if bytes.len() < LEN_ENDPOINT_DESCRIPTOR as usize {
            return Err(UsbDescriptorError::BufferTooShort {
                expected: LEN_ENDPOINT_DESCRIPTOR as usize,
                actual: bytes.len(),
            });
        }
        let b_length = bytes[0];
        let b_descriptor_type = bytes[1];
        if b_length < LEN_ENDPOINT_DESCRIPTOR {
            return Err(UsbDescriptorError::InvalidLength {
                descriptor_type: b_descriptor_type,
                length: b_length,
            });
        }
        if b_descriptor_type != DESC_TYPE_ENDPOINT {
            return Err(UsbDescriptorError::InvalidType {
                expected: DESC_TYPE_ENDPOINT,
                actual: b_descriptor_type,
            });
        }

        let mut rdr = Cursor::new(bytes);
        let _ = rdr.read_u8().map_err(|e| UsbDescriptorError::MalformedDescriptor(e.to_string()))?;
        let _ = rdr.read_u8().map_err(|e| UsbDescriptorError::MalformedDescriptor(e.to_string()))?;
        let b_endpoint_address = rdr.read_u8().map_err(|e| UsbDescriptorError::MalformedDescriptor(e.to_string()))?;
        let bm_attributes = rdr.read_u8().map_err(|e| UsbDescriptorError::MalformedDescriptor(e.to_string()))?;
        let w_max_packet_size = rdr.read_u16::<LittleEndian>().map_err(|e| UsbDescriptorError::MalformedDescriptor(e.to_string()))?;
        let b_interval = rdr.read_u8().map_err(|e| UsbDescriptorError::MalformedDescriptor(e.to_string()))?;

        Ok(Self {
            b_length,
            b_descriptor_type,
            b_endpoint_address,
            bm_attributes,
            w_max_packet_size,
            b_interval,
        })
    }

    #[inline]
    pub fn endpoint_number(&self) -> u8 {
        self.b_endpoint_address & 0x0F
    }

    #[inline]
    pub fn is_in(&self) -> bool {
        (self.b_endpoint_address & 0x80) != 0
    }

    #[inline]
    pub fn is_out(&self) -> bool {
        (self.b_endpoint_address & 0x80) == 0
    }

    #[inline]
    pub fn direction(&self) -> EndpointDirection {
        if self.is_in() {
            EndpointDirection::In
        } else {
            EndpointDirection::Out
        }
    }

    #[inline]
    pub fn transfer_type(&self) -> EndpointTransferType {
        match self.bm_attributes & 0x03 {
            0 => EndpointTransferType::Control,
            1 => EndpointTransferType::Isochronous,
            2 => EndpointTransferType::Bulk,
            3 => EndpointTransferType::Interrupt,
            _ => unreachable!(),
        }
    }

    #[inline]
    pub fn max_packet_size(&self) -> u16 {
        self.w_max_packet_size & 0x07FF
    }

    #[inline]
    pub fn additional_transactions_per_microframe(&self) -> u8 {
        ((self.w_max_packet_size >> 11) & 0x03) as u8
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        let mut buf = Vec::with_capacity(LEN_ENDPOINT_DESCRIPTOR as usize);
        buf.write_u8(self.b_length).unwrap();
        buf.write_u8(self.b_descriptor_type).unwrap();
        buf.write_u8(self.b_endpoint_address).unwrap();
        buf.write_u8(self.bm_attributes).unwrap();
        buf.write_u16::<LittleEndian>(self.w_max_packet_size).unwrap();
        buf.write_u8(self.b_interval).unwrap();
        buf
    }
}

// -----------------------------------------------------------------------------
// 5. USB 3.0 SuperSpeed Endpoint Companion Descriptor (USB 3.2 Spec §9.6.7)
// -----------------------------------------------------------------------------
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UsbSsEndpointCompanionDescriptor {
    pub b_length: u8,
    pub b_descriptor_type: u8,
    pub b_max_burst: u8,
    pub bm_attributes: u8,
    pub w_bytes_per_interval: u16,
}

impl UsbSsEndpointCompanionDescriptor {
    pub fn parse(bytes: &[u8]) -> Result<Self, UsbDescriptorError> {
        if bytes.len() < LEN_SS_ENDPOINT_COMPANION as usize {
            return Err(UsbDescriptorError::BufferTooShort {
                expected: LEN_SS_ENDPOINT_COMPANION as usize,
                actual: bytes.len(),
            });
        }
        let b_length = bytes[0];
        let b_descriptor_type = bytes[1];
        if b_length < LEN_SS_ENDPOINT_COMPANION {
            return Err(UsbDescriptorError::InvalidLength {
                descriptor_type: b_descriptor_type,
                length: b_length,
            });
        }
        if b_descriptor_type != DESC_TYPE_SS_ENDPOINT_COMPANION {
            return Err(UsbDescriptorError::InvalidType {
                expected: DESC_TYPE_SS_ENDPOINT_COMPANION,
                actual: b_descriptor_type,
            });
        }

        let mut rdr = Cursor::new(bytes);
        let _ = rdr.read_u8().map_err(|e| UsbDescriptorError::MalformedDescriptor(e.to_string()))?;
        let _ = rdr.read_u8().map_err(|e| UsbDescriptorError::MalformedDescriptor(e.to_string()))?;
        let b_max_burst = rdr.read_u8().map_err(|e| UsbDescriptorError::MalformedDescriptor(e.to_string()))?;
        let bm_attributes = rdr.read_u8().map_err(|e| UsbDescriptorError::MalformedDescriptor(e.to_string()))?;
        let w_bytes_per_interval = rdr.read_u16::<LittleEndian>().map_err(|e| UsbDescriptorError::MalformedDescriptor(e.to_string()))?;

        Ok(Self {
            b_length,
            b_descriptor_type,
            b_max_burst,
            bm_attributes,
            w_bytes_per_interval,
        })
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        let mut buf = Vec::with_capacity(LEN_SS_ENDPOINT_COMPANION as usize);
        buf.write_u8(self.b_length).unwrap();
        buf.write_u8(self.b_descriptor_type).unwrap();
        buf.write_u8(self.b_max_burst).unwrap();
        buf.write_u8(self.bm_attributes).unwrap();
        buf.write_u16::<LittleEndian>(self.w_bytes_per_interval).unwrap();
        buf
    }
}

// -----------------------------------------------------------------------------
// 6. USB Interface Association Descriptor (IAD) (USB ECN)
// -----------------------------------------------------------------------------
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UsbInterfaceAssociationDescriptor {
    pub b_length: u8,
    pub b_descriptor_type: u8,
    pub b_first_interface: u8,
    pub b_interface_count: u8,
    pub b_function_class: u8,
    pub b_function_sub_class: u8,
    pub b_function_protocol: u8,
    pub i_function: u8,
}

impl UsbInterfaceAssociationDescriptor {
    pub fn parse(bytes: &[u8]) -> Result<Self, UsbDescriptorError> {
        if bytes.len() < LEN_IAD_DESCRIPTOR as usize {
            return Err(UsbDescriptorError::BufferTooShort {
                expected: LEN_IAD_DESCRIPTOR as usize,
                actual: bytes.len(),
            });
        }
        let b_length = bytes[0];
        let b_descriptor_type = bytes[1];
        if b_length < LEN_IAD_DESCRIPTOR {
            return Err(UsbDescriptorError::InvalidLength {
                descriptor_type: b_descriptor_type,
                length: b_length,
            });
        }
        if b_descriptor_type != DESC_TYPE_INTERFACE_ASSOCIATION {
            return Err(UsbDescriptorError::InvalidType {
                expected: DESC_TYPE_INTERFACE_ASSOCIATION,
                actual: b_descriptor_type,
            });
        }

        let mut rdr = Cursor::new(bytes);
        let _ = rdr.read_u8().map_err(|e| UsbDescriptorError::MalformedDescriptor(e.to_string()))?;
        let _ = rdr.read_u8().map_err(|e| UsbDescriptorError::MalformedDescriptor(e.to_string()))?;
        let b_first_interface = rdr.read_u8().map_err(|e| UsbDescriptorError::MalformedDescriptor(e.to_string()))?;
        let b_interface_count = rdr.read_u8().map_err(|e| UsbDescriptorError::MalformedDescriptor(e.to_string()))?;
        let b_function_class = rdr.read_u8().map_err(|e| UsbDescriptorError::MalformedDescriptor(e.to_string()))?;
        let b_function_sub_class = rdr.read_u8().map_err(|e| UsbDescriptorError::MalformedDescriptor(e.to_string()))?;
        let b_function_protocol = rdr.read_u8().map_err(|e| UsbDescriptorError::MalformedDescriptor(e.to_string()))?;
        let i_function = rdr.read_u8().map_err(|e| UsbDescriptorError::MalformedDescriptor(e.to_string()))?;

        Ok(Self {
            b_length,
            b_descriptor_type,
            b_first_interface,
            b_interface_count,
            b_function_class,
            b_function_sub_class,
            b_function_protocol,
            i_function,
        })
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        let mut buf = Vec::with_capacity(LEN_IAD_DESCRIPTOR as usize);
        buf.write_u8(self.b_length).unwrap();
        buf.write_u8(self.b_descriptor_type).unwrap();
        buf.write_u8(self.b_first_interface).unwrap();
        buf.write_u8(self.b_interface_count).unwrap();
        buf.write_u8(self.b_function_class).unwrap();
        buf.write_u8(self.b_function_sub_class).unwrap();
        buf.write_u8(self.b_function_protocol).unwrap();
        buf.write_u8(self.i_function).unwrap();
        buf
    }
}

// -----------------------------------------------------------------------------
// 7. USB HID Descriptor (Device Class Definition for HID v1.11 §6.2.1)
// -----------------------------------------------------------------------------
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UsbHidDescriptor {
    pub b_length: u8,
    pub b_descriptor_type: u8,
    pub bcd_hid: u16,
    pub b_country_code: u8,
    pub b_num_descriptors: u8,
    pub class_descriptors: Vec<(u8 /* type */, u16 /* length */)>,
}

impl UsbHidDescriptor {
    pub fn parse(bytes: &[u8]) -> Result<Self, UsbDescriptorError> {
        if bytes.len() < 9 {
            return Err(UsbDescriptorError::BufferTooShort {
                expected: 9,
                actual: bytes.len(),
            });
        }
        let b_length = bytes[0];
        let b_descriptor_type = bytes[1];
        if b_length < 9 {
            return Err(UsbDescriptorError::InvalidLength {
                descriptor_type: b_descriptor_type,
                length: b_length,
            });
        }
        if b_descriptor_type != DESC_TYPE_HID {
            return Err(UsbDescriptorError::InvalidType {
                expected: DESC_TYPE_HID,
                actual: b_descriptor_type,
            });
        }

        let mut rdr = Cursor::new(bytes);
        let _ = rdr.read_u8().map_err(|e| UsbDescriptorError::MalformedDescriptor(e.to_string()))?;
        let _ = rdr.read_u8().map_err(|e| UsbDescriptorError::MalformedDescriptor(e.to_string()))?;
        let bcd_hid = rdr.read_u16::<LittleEndian>().map_err(|e| UsbDescriptorError::MalformedDescriptor(e.to_string()))?;
        let b_country_code = rdr.read_u8().map_err(|e| UsbDescriptorError::MalformedDescriptor(e.to_string()))?;
        let b_num_descriptors = rdr.read_u8().map_err(|e| UsbDescriptorError::MalformedDescriptor(e.to_string()))?;

        let mut class_descriptors = Vec::with_capacity(b_num_descriptors as usize);
        for _ in 0..b_num_descriptors {
            let desc_type = rdr.read_u8().map_err(|e| UsbDescriptorError::MalformedDescriptor(e.to_string()))?;
            let desc_len = rdr.read_u16::<LittleEndian>().map_err(|e| UsbDescriptorError::MalformedDescriptor(e.to_string()))?;
            class_descriptors.push((desc_type, desc_len));
        }

        Ok(Self {
            b_length,
            b_descriptor_type,
            bcd_hid,
            b_country_code,
            b_num_descriptors,
            class_descriptors,
        })
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        let mut buf = Vec::with_capacity(self.b_length as usize);
        buf.write_u8(self.b_length).unwrap();
        buf.write_u8(self.b_descriptor_type).unwrap();
        buf.write_u16::<LittleEndian>(self.bcd_hid).unwrap();
        buf.write_u8(self.b_country_code).unwrap();
        buf.write_u8(self.b_num_descriptors).unwrap();
        for &(desc_type, desc_len) in &self.class_descriptors {
            buf.write_u8(desc_type).unwrap();
            buf.write_u16::<LittleEndian>(desc_len).unwrap();
        }
        buf
    }
}

// -----------------------------------------------------------------------------
// 8. HID Report Descriptor Parser Item Representation (HID Spec §6.2.2)
// -----------------------------------------------------------------------------
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum HidItemType {
    Main,
    Global,
    Local,
    Reserved,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HidReportItem {
    pub item_type: HidItemType,
    pub tag: u8,
    pub size: u8,
    pub data: u32,
    pub raw_bytes: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct UsbHidReportDescriptorSummary {
    pub total_items: usize,
    pub usages: Vec<u32>,
    pub report_ids: Vec<u8>,
    pub has_keyboard: bool,
    pub has_mouse: bool,
    pub has_biometric: bool,
    pub items: Vec<HidReportItem>,
}

pub fn parse_hid_report_descriptor(raw: &[u8]) -> Result<UsbHidReportDescriptorSummary, UsbDescriptorError> {
    let mut summary = UsbHidReportDescriptorSummary::default();
    let mut idx = 0;

    while idx < raw.len() {
        let prefix = raw[idx];
        idx += 1;

        // Long item check
        if prefix == 0xFE {
            if idx + 2 > raw.len() {
                return Err(UsbDescriptorError::MalformedDescriptor("Truncated HID long item".into()));
            }
            let data_size = raw[idx] as usize;
            let tag = raw[idx + 1];
            idx += 2;
            if idx + data_size > raw.len() {
                return Err(UsbDescriptorError::MalformedDescriptor("Truncated HID long item data".into()));
            }
            let data_bytes = raw[idx..idx + data_size].to_vec();
            idx += data_size;
            summary.items.push(HidReportItem {
                item_type: HidItemType::Reserved,
                tag,
                size: data_size as u8,
                data: 0,
                raw_bytes: data_bytes,
            });
            summary.total_items += 1;
            continue;
        }

        // Short item:
        // bit 0-1: Size (0 = 0 bytes, 1 = 1 byte, 2 = 2 bytes, 3 = 4 bytes)
        // bit 2-3: Type (0 = Main, 1 = Global, 2 = Local, 3 = Reserved)
        // bit 4-7: Tag
        let size_code = prefix & 0x03;
        let data_size = match size_code {
            0 => 0,
            1 => 1,
            2 => 2,
            3 => 4,
            _ => unreachable!(),
        };
        let type_code = (prefix >> 2) & 0x03;
        let tag = (prefix >> 4) & 0x0F;

        let item_type = match type_code {
            0 => HidItemType::Main,
            1 => HidItemType::Global,
            2 => HidItemType::Local,
            _ => HidItemType::Reserved,
        };

        if idx + data_size > raw.len() {
            return Err(UsbDescriptorError::MalformedDescriptor(format!(
                "Truncated HID short item at offset {}, needed {} bytes",
                idx, data_size
            )));
        }

        let item_bytes = raw[idx..idx + data_size].to_vec();
        let mut data: u32 = 0;
        for (i, &b) in item_bytes.iter().enumerate() {
            data |= (b as u32) << (i * 8);
        }
        idx += data_size;

        // Extract semantic meaning for common usages
        // Global: Usage Page (tag = 0x00)
        // Local: Usage (tag = 0x00)
        // Global: Report ID (tag = 0x08)
        if item_type == HidItemType::Global && tag == 0x08 {
            summary.report_ids.push(data as u8);
        } else if item_type == HidItemType::Local && tag == 0x00 {
            summary.usages.push(data);
            match data {
                0x010006 | 0x06 => summary.has_keyboard = true, // Keyboard
                0x010002 | 0x02 => summary.has_mouse = true,    // Mouse
                0x00100000..=0x0010FFFF => summary.has_biometric = true, // Biometric page
                _ => {}
            }
        }

        summary.items.push(HidReportItem {
            item_type,
            tag,
            size: data_size as u8,
            data,
            raw_bytes: item_bytes,
        });
        summary.total_items += 1;
    }

    Ok(summary)
}

// -----------------------------------------------------------------------------
// 9. Raw Generic Descriptor Header
// -----------------------------------------------------------------------------
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RawDescriptorHeader {
    pub b_length: u8,
    pub b_descriptor_type: u8,
    pub data: Vec<u8>,
}

impl RawDescriptorHeader {
    pub fn parse(bytes: &[u8]) -> Result<(Self, usize), UsbDescriptorError> {
        if bytes.len() < 2 {
            return Err(UsbDescriptorError::BufferTooShort {
                expected: 2,
                actual: bytes.len(),
            });
        }
        let b_length = bytes[0];
        let b_descriptor_type = bytes[1];
        if b_length < 2 {
            return Err(UsbDescriptorError::InvalidLength {
                descriptor_type: b_descriptor_type,
                length: b_length,
            });
        }
        if bytes.len() < b_length as usize {
            return Err(UsbDescriptorError::BufferTooShort {
                expected: b_length as usize,
                actual: bytes.len(),
            });
        }
        let data = bytes[2..b_length as usize].to_vec();
        Ok((
            Self {
                b_length,
                b_descriptor_type,
                data,
            },
            b_length as usize,
        ))
    }
}

// -----------------------------------------------------------------------------
// 10. Parsed AST Tree for a USB Configuration
// -----------------------------------------------------------------------------
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UsbParsedEndpoint {
    pub descriptor: UsbEndpointDescriptor,
    pub ss_companion: Option<UsbSsEndpointCompanionDescriptor>,
    pub extra: Vec<RawDescriptorHeader>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UsbParsedInterface {
    pub descriptor: UsbInterfaceDescriptor,
    pub hid_descriptor: Option<UsbHidDescriptor>,
    pub endpoints: Vec<UsbParsedEndpoint>,
    pub class_specific: Vec<RawDescriptorHeader>,
    pub extra: Vec<RawDescriptorHeader>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UsbParsedConfiguration {
    pub descriptor: UsbConfigurationDescriptor,
    pub iad_list: Vec<UsbInterfaceAssociationDescriptor>,
    pub interfaces: Vec<UsbParsedInterface>,
    pub extra: Vec<RawDescriptorHeader>,
}

impl UsbParsedConfiguration {
    /// Full recursive parser for a Configuration Descriptor and all attached sub-descriptors
    pub fn parse_complete(bytes: &[u8]) -> Result<Self, UsbDescriptorError> {
        if bytes.len() < LEN_CONFIGURATION_DESCRIPTOR as usize {
            return Err(UsbDescriptorError::BufferTooShort {
                expected: LEN_CONFIGURATION_DESCRIPTOR as usize,
                actual: bytes.len(),
            });
        }
        let config_desc = UsbConfigurationDescriptor::parse(bytes)?;
        let total_length = (config_desc.w_total_length as usize).min(bytes.len());

        let mut offset = config_desc.b_length as usize;
        let mut iad_list = Vec::new();
        let mut interfaces = Vec::new();
        let mut extra = Vec::new();

        let mut current_iface: Option<UsbParsedInterface> = None;
        let mut current_ep: Option<UsbParsedEndpoint> = None;

        while offset + 2 <= total_length {
            let desc_len = bytes[offset] as usize;
            let desc_type = bytes[offset + 1];

            if desc_len < 2 || offset + desc_len > total_length {
                break;
            }

            let desc_slice = &bytes[offset..offset + desc_len];

            match desc_type {
                DESC_TYPE_INTERFACE_ASSOCIATION => {
                    if let Ok(iad) = UsbInterfaceAssociationDescriptor::parse(desc_slice) {
                        iad_list.push(iad);
                    }
                }
                DESC_TYPE_INTERFACE => {
                    // Flush current endpoint and interface
                    if let Some(mut iface) = current_iface.take() {
                        if let Some(ep) = current_ep.take() {
                            iface.endpoints.push(ep);
                        }
                        interfaces.push(iface);
                    }
                    if let Ok(iface_desc) = UsbInterfaceDescriptor::parse(desc_slice) {
                        current_iface = Some(UsbParsedInterface {
                            descriptor: iface_desc,
                            hid_descriptor: None,
                            endpoints: Vec::new(),
                            class_specific: Vec::new(),
                            extra: Vec::new(),
                        });
                    }
                }
                DESC_TYPE_ENDPOINT => {
                    if let Some(ref mut iface) = current_iface {
                        if let Some(ep) = current_ep.take() {
                            iface.endpoints.push(ep);
                        }
                        if let Ok(ep_desc) = UsbEndpointDescriptor::parse(desc_slice) {
                            current_ep = Some(UsbParsedEndpoint {
                                descriptor: ep_desc,
                                ss_companion: None,
                                extra: Vec::new(),
                            });
                        }
                    }
                }
                DESC_TYPE_SS_ENDPOINT_COMPANION => {
                    if let Some(ref mut ep) = current_ep {
                        if let Ok(comp) = UsbSsEndpointCompanionDescriptor::parse(desc_slice) {
                            ep.ss_companion = Some(comp);
                        }
                    }
                }
                DESC_TYPE_HID => {
                    if let Some(ref mut iface) = current_iface {
                        if let Ok(hid) = UsbHidDescriptor::parse(desc_slice) {
                            iface.hid_descriptor = Some(hid);
                        }
                    }
                }
                DESC_TYPE_CS_INTERFACE | DESC_TYPE_CS_ENDPOINT => {
                    let (raw, _) = RawDescriptorHeader::parse(desc_slice)?;
                    if let Some(ref mut ep) = current_ep {
                        ep.extra.push(raw);
                    } else if let Some(ref mut iface) = current_iface {
                        iface.class_specific.push(raw);
                    }
                }
                _ => {
                    let (raw, _) = RawDescriptorHeader::parse(desc_slice)?;
                    if let Some(ref mut ep) = current_ep {
                        ep.extra.push(raw);
                    } else if let Some(ref mut iface) = current_iface {
                        iface.extra.push(raw);
                    } else {
                        extra.push(raw);
                    }
                }
            }

            offset += desc_len;
        }

        // Flush tail
        if let Some(mut iface) = current_iface {
            if let Some(ep) = current_ep {
                iface.endpoints.push(ep);
            }
            interfaces.push(iface);
        }

        Ok(Self {
            descriptor: config_desc,
            iad_list,
            interfaces,
            extra,
        })
    }

    /// Serializes the entire configuration AST back to standard USB descriptor binary stream
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut buf = Vec::new();
        // Reserve 9 bytes for configuration descriptor header; will overwrite wTotalLength at end
        buf.extend_from_slice(&self.descriptor.to_bytes());

        for iad in &self.iad_list {
            buf.extend_from_slice(&iad.to_bytes());
        }

        for extra_desc in &self.extra {
            buf.write_u8(extra_desc.b_length).unwrap();
            buf.write_u8(extra_desc.b_descriptor_type).unwrap();
            buf.extend_from_slice(&extra_desc.data);
        }

        for iface in &self.interfaces {
            buf.extend_from_slice(&iface.descriptor.to_bytes());

            if let Some(ref hid) = iface.hid_descriptor {
                buf.extend_from_slice(&hid.to_bytes());
            }

            for cs in &iface.class_specific {
                buf.write_u8(cs.b_length).unwrap();
                buf.write_u8(cs.b_descriptor_type).unwrap();
                buf.extend_from_slice(&cs.data);
            }

            for extra_desc in &iface.extra {
                buf.write_u8(extra_desc.b_length).unwrap();
                buf.write_u8(extra_desc.b_descriptor_type).unwrap();
                buf.extend_from_slice(&extra_desc.data);
            }

            for ep in &iface.endpoints {
                buf.extend_from_slice(&ep.descriptor.to_bytes());

                if let Some(ref ss) = ep.ss_companion {
                    buf.extend_from_slice(&ss.to_bytes());
                }

                for ep_extra in &ep.extra {
                    buf.write_u8(ep_extra.b_length).unwrap();
                    buf.write_u8(ep_extra.b_descriptor_type).unwrap();
                    buf.extend_from_slice(&ep_extra.data);
                }
            }
        }

        // Patch wTotalLength at offset 2..4
        let total_len = buf.len() as u16;
        buf[2] = (total_len & 0xFF) as u8;
        buf[3] = ((total_len >> 8) & 0xFF) as u8;

        buf
    }
}

// -----------------------------------------------------------------------------
// 11. USBD_ParseConfigurationDescriptorEx Function-by-Function Implementation
// -----------------------------------------------------------------------------
/// Equivalent to the Windows USBD API:
/// `USBD_ParseConfigurationDescriptorEx(ConfigurationDescriptor, StartPosition, InterfaceNumber, AlternateSetting, InterfaceClass, InterfaceSubClass, InterfaceProtocol)`
///
/// Parameters set to `None` or `< 0` match ANY value (wildcard / don't care).
/// Returns `Some((offset, UsbInterfaceDescriptor))` of the first matching interface, or `None`.
pub fn usbd_parse_configuration_descriptor_ex(
    raw_descriptors: &[u8],
    start_offset: usize,
    interface_number: Option<i32>,
    alternate_setting: Option<i32>,
    interface_class: Option<i32>,
    interface_sub_class: Option<i32>,
    interface_protocol: Option<i32>,
) -> Option<(usize, UsbInterfaceDescriptor)> {
    if raw_descriptors.len() < LEN_CONFIGURATION_DESCRIPTOR as usize || start_offset >= raw_descriptors.len() {
        return None;
    }

    let mut offset = start_offset;
    while offset + 2 <= raw_descriptors.len() {
        let b_length = raw_descriptors[offset] as usize;
        let b_descriptor_type = raw_descriptors[offset + 1];

        if b_length < 2 || offset + b_length > raw_descriptors.len() {
            break;
        }

        if b_descriptor_type == DESC_TYPE_INTERFACE && b_length >= LEN_INTERFACE_DESCRIPTOR as usize {
            if let Ok(iface) = UsbInterfaceDescriptor::parse(&raw_descriptors[offset..offset + b_length]) {
                let match_num = interface_number.map_or(true, |v| v < 0 || v as u8 == iface.b_interface_number);
                let match_alt = alternate_setting.map_or(true, |v| v < 0 || v as u8 == iface.b_alternate_setting);
                let match_cls = interface_class.map_or(true, |v| v < 0 || v as u8 == iface.b_interface_class);
                let match_sub = interface_sub_class.map_or(true, |v| v < 0 || v as u8 == iface.b_interface_sub_class);
                let match_prt = interface_protocol.map_or(true, |v| v < 0 || v as u8 == iface.b_interface_protocol);

                if match_num && match_alt && match_cls && match_sub && match_prt {
                    return Some((offset, iface));
                }
            }
        }

        offset += b_length;
    }

    None
}

// -----------------------------------------------------------------------------
// 12. USBD_ParseDescriptors Search Utility
// -----------------------------------------------------------------------------
/// Equivalent to Windows USBD `USBD_ParseDescriptors` to find descriptors of a specific type.
pub fn usbd_parse_descriptors<'a>(
    raw_descriptors: &'a [u8],
    start_offset: usize,
    descriptor_type: u8,
) -> Option<(usize, &'a [u8])> {
    let mut offset = start_offset;
    while offset + 2 <= raw_descriptors.len() {
        let b_length = raw_descriptors[offset] as usize;
        let b_type = raw_descriptors[offset + 1];

        if b_length < 2 || offset + b_length > raw_descriptors.len() {
            break;
        }

        if b_type == descriptor_type {
            return Some((offset, &raw_descriptors[offset..offset + b_length]));
        }

        offset += b_length;
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_device_descriptor_roundtrip() {
        // Mantra MFS100 Device Descriptor (VID 0x27D0, PID 0x0041)
        let raw = [
            0x12, 0x01,       // bLength=18, bDescriptorType=DEVICE
            0x00, 0x02,       // bcdUSB=2.00
            0x00, 0x00, 0x00, // class=0, subclass=0, protocol=0
            0x40,             // bMaxPacketSize0=64
            0xD0, 0x27,       // idVendor=0x27D0
            0x41, 0x00,       // idProduct=0x0041
            0x00, 0x01,       // bcdDevice=1.00
            0x01, 0x02, 0x03, // iManufacturer=1, iProduct=2, iSerialNumber=3
            0x01,             // bNumConfigurations=1
        ];

        let desc = UsbDeviceDescriptor::parse(&raw).expect("Must parse MFS100 device descriptor");
        assert_eq!(desc.id_vendor, 0x27D0);
        assert_eq!(desc.id_product, 0x0041);
        assert_eq!(desc.b_max_packet_size0, 64);
        assert_eq!(desc.b_num_configurations, 1);

        let roundtrip = desc.to_bytes();
        assert_eq!(&raw[..], &roundtrip[..]);
    }

    #[test]
    fn test_configuration_tree_and_usbd_search() {
        // Build a complete configuration blob:
        // Config -> Interface 0 (Alt 0) -> EP1 IN Bulk, EP1 OUT Bulk
        //        -> Interface 1 (Alt 0) -> HID Desc -> EP2 IN Interrupt
        let config = UsbParsedConfiguration {
            descriptor: UsbConfigurationDescriptor {
                b_length: 9,
                b_descriptor_type: DESC_TYPE_CONFIGURATION,
                w_total_length: 0, // will be auto-calculated
                b_num_interfaces: 2,
                b_configuration_value: 1,
                i_configuration: 0,
                bm_attributes: 0xC0, // Self-powered
                b_max_power: 50,     // 100mA
            },
            iad_list: Vec::new(),
            interfaces: vec![
                UsbParsedInterface {
                    descriptor: UsbInterfaceDescriptor {
                        b_length: 9,
                        b_descriptor_type: DESC_TYPE_INTERFACE,
                        b_interface_number: 0,
                        b_alternate_setting: 0,
                        b_num_endpoints: 2,
                        b_interface_class: 0xFF, // Vendor specific
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
                                b_endpoint_address: 0x81, // EP1 IN
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
                                b_endpoint_address: 0x01, // EP1 OUT
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
                },
                UsbParsedInterface {
                    descriptor: UsbInterfaceDescriptor {
                        b_length: 9,
                        b_descriptor_type: DESC_TYPE_INTERFACE,
                        b_interface_number: 1,
                        b_alternate_setting: 0,
                        b_num_endpoints: 1,
                        b_interface_class: 0x03, // HID Class
                        b_interface_sub_class: 0x01,
                        b_interface_protocol: 0x02,
                        i_interface: 0,
                    },
                    hid_descriptor: Some(UsbHidDescriptor {
                        b_length: 9,
                        b_descriptor_type: DESC_TYPE_HID,
                        bcd_hid: 0x0111,
                        b_country_code: 0,
                        b_num_descriptors: 1,
                        class_descriptors: vec![(DESC_TYPE_HID_REPORT, 65)],
                    }),
                    endpoints: vec![UsbParsedEndpoint {
                        descriptor: UsbEndpointDescriptor {
                            b_length: 7,
                            b_descriptor_type: DESC_TYPE_ENDPOINT,
                            b_endpoint_address: 0x82, // EP2 IN
                            bm_attributes: 0x03,       // Interrupt
                            w_max_packet_size: 64,
                            b_interval: 10,
                        },
                        ss_companion: None,
                        extra: Vec::new(),
                    }],
                    class_specific: Vec::new(),
                    extra: Vec::new(),
                },
            ],
            extra: Vec::new(),
        };

        let raw_blob = config.to_bytes();
        assert!(raw_blob.len() > 40);

        // Parse back
        let parsed_tree = UsbParsedConfiguration::parse_complete(&raw_blob).expect("Must parse tree");
        assert_eq!(parsed_tree.interfaces.len(), 2);
        assert_eq!(parsed_tree.interfaces[0].endpoints.len(), 2);
        assert!(parsed_tree.interfaces[0].endpoints[0].descriptor.is_in());
        assert!(parsed_tree.interfaces[0].endpoints[1].descriptor.is_out());
        assert!(parsed_tree.interfaces[1].hid_descriptor.is_some());

        // Test USBD_ParseConfigurationDescriptorEx on Interface 1 (HID)
        let found = usbd_parse_configuration_descriptor_ex(
            &raw_blob,
            0,
            Some(1),
            Some(0),
            Some(0x03),
            None,
            None,
        );
        assert!(found.is_some());
        let (_offset, iface) = found.unwrap();
        assert_eq!(iface.b_interface_number, 1);
        assert_eq!(iface.b_interface_class, 0x03);

        // Search for non-existent interface
        let missing = usbd_parse_configuration_descriptor_ex(
            &raw_blob,
            0,
            Some(5),
            None,
            None,
            None,
            None,
        );
        assert!(missing.is_none());
    }

    #[test]
    fn test_hid_report_descriptor_parsing() {
        // Standard 3-button Mouse Report Descriptor (50 bytes)
        let mouse_report: [u8; 50] = [
            0x05, 0x01, // Usage Page (Generic Desktop)
            0x09, 0x02, // Usage (Mouse)
            0xA1, 0x01, // Collection (Application)
            0x09, 0x01, //   Usage (Pointer)
            0xA1, 0x00, //   Collection (Physical)
            0x05, 0x09, //     Usage Page (Buttons)
            0x19, 0x01, //     Usage Minimum (1)
            0x29, 0x03, //     Usage Maximum (3)
            0x15, 0x00, //     Logical Minimum (0)
            0x25, 0x01, //     Logical Maximum (1)
            0x95, 0x03, //     Report Count (3)
            0x75, 0x01, //     Report Size (1)
            0x81, 0x02, //     Input (Data, Variable, Absolute)
            0x95, 0x01, //     Report Count (1)
            0x75, 0x05, //     Report Size (5)
            0x81, 0x01, //     Input (Constant)
            0x05, 0x01, //     Usage Page (Generic Desktop)
            0x09, 0x30, //     Usage (X)
            0x09, 0x31, //     Usage (Y)
            0x15, 0x81, //     Logical Minimum (-127)
            0x25, 0x7F, //     Logical Maximum (127)
            0x75, 0x08, //     Report Size (8)
            0x95, 0x02, //     Report Count (2)
            0x81, 0x06, //     Input (Data, Variable, Relative)
            0xC0,       //   End Collection
            0xC0,       // End Collection
        ];

        let summary = parse_hid_report_descriptor(&mouse_report).expect("Must parse mouse report");
        assert!(summary.has_mouse);
        assert!(!summary.has_keyboard);
        assert!(summary.total_items > 10);
    }
}
