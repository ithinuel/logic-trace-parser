#![allow(dead_code)]

use super::cdc;
use super::msd;

use super::lang_id::*;

//use crate::usb::types::*;

use itertools::Itertools;
use std::convert::From;
use std::convert::TryFrom;
use std::convert::TryInto;

const DEVICE_DESCRIPTOR: u8 = 1;
const CONFIGURATION_DESCRIPTOR: u8 = 2;
const STRING_DESCRIPTOR: u8 = 3;
const INTERFACE_DESCRIPTOR: u8 = 4;
const ENDPOINT_DESCRIPTOR: u8 = 5;
const DEVICE_QUALIFIER_DESCRIPTOR: u8 = 6;
const OTHER_SPEED_CONFIGURATION_DESCRIPTOR: u8 = 7;
const INTERFACE_POWER_DESCRIPTOR: u8 = 8;
const OTG_DESCRIPTOR: u8 = 9;
const DEBUG_DESCRIPTOR: u8 = 10;
const INTERFACE_ASSOCIATION_DESCRIPTOR: u8 = 11;
const BINARY_OBJECT_STORE_DESCRIPTOR: u8 = 15;
const DEVICE_CAPABILITY_DESCRIPTOR: u8 = 16;

#[derive(PartialEq, Debug, Copy, Clone)]
pub enum Recipient {
    Device,
    Interface,
    Endpoint,
    Other,
    Reserved(u8),
}

#[derive(PartialEq, Debug, Copy, Clone)]
pub enum RequestType {
    Standard,
    Class,
    Vendor,
    Reserved,
}

#[derive(PartialEq, Debug, Copy, Clone)]
pub enum DataPhaseTransferDirection {
    In,
    Out,
}

struct PackedRequestType(u8);
impl PackedRequestType {
    pub fn direction(&self) -> DataPhaseTransferDirection {
        if (self.0 & 0x80) == 0x80 {
            DataPhaseTransferDirection::In
        } else {
            DataPhaseTransferDirection::Out
        }
    }

    pub fn request_type(&self) -> RequestType {
        let ptype = (self.0 >> 5) & 0b0000_0011;
        if ptype == 0 {
            RequestType::Standard
        } else if ptype == 1 {
            RequestType::Class
        } else if ptype == 2 {
            RequestType::Vendor
        } else {
            RequestType::Reserved
        }
    }

    pub fn recipient(&self) -> Recipient {
        match self.0 & 0x1F {
            0 => Recipient::Device,
            1 => Recipient::Interface,
            2 => Recipient::Endpoint,
            3 => Recipient::Other,
            b => Recipient::Reserved(b),
        }
    }
}

#[derive(Debug, Copy, Clone, PartialEq)]
pub enum DeviceRequest {
    Standard(StandardRequest),
    Class(),
    Vendor(),
    Reserved { request: u8, value: u16, index: u16 },
}

impl TryFrom<(RequestType, u8, u16, u16)> for DeviceRequest {
    type Error = anyhow::Error;

    fn try_from(value: (RequestType, u8, u16, u16)) -> Result<Self, Self::Error> {
        let (request_type, request, value, index) = value;

        Ok(match request_type {
            RequestType::Reserved => Self::Reserved {
                request,
                value,
                index,
            },
            RequestType::Standard => {
                Self::Standard(StandardRequest::try_from((request, value, index))?)
            }
            _ => anyhow::bail!("unsupported"),
        })
    }
}

#[derive(Debug, Copy, Clone, PartialEq)]
pub enum ERequest {
    Device(DeviceRequest),
    Interface {
        request_type: RequestType,
        request: u8,
        value: u16,
        index: u16,
    },
    Endpoint(),
    Unknown {
        recipient: u8,
        request_type: RequestType,
        request: u8,
        value: u16,
        index: u16,
    },
}

/// try from (recipient, request_type, request, value, index)
impl TryFrom<(Recipient, RequestType, u8, u16, u16)> for ERequest {
    type Error = anyhow::Error;

    fn try_from(value: (Recipient, RequestType, u8, u16, u16)) -> Result<Self, Self::Error> {
        let (recipient, request_type, request, value, index) = value;

        Ok(match recipient {
            Recipient::Reserved(n) => Self::Unknown {
                recipient: n,
                request_type,
                request,
                value,
                index,
            },
            Recipient::Interface => Self::Interface {
                request_type,
                request,
                value,
                index,
            },
            Recipient::Device => Self::Device(DeviceRequest::try_from((
                request_type,
                request,
                value,
                index,
            ))?),
            _ => anyhow::bail!("not implemented: {:?}", recipient),
        })
    }
}

#[derive(PartialEq, Clone, Copy)]
pub struct Request {
    pub direction: DataPhaseTransferDirection,
    pub request: ERequest,
    pub length: u16,
}
impl std::fmt::Debug for Request {
    fn fmt<'a>(&'a self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DeviceRequest")
            .field("direction", &self.direction)
            .field("request", &self.request)
            .field("length", &self.length)
            .finish()
    }
}
impl TryFrom<&[u8]> for Request {
    type Error = anyhow::Error;
    fn try_from(from: &[u8]) -> Result<Request, Self::Error> {
        anyhow::ensure!(from.len() == 8, "Invalid Setup packet length.");
        let packed_request_type = PackedRequestType(from[0]);
        let value = from[2..4].try_into()?;
        let index = from[4..6].try_into()?;
        let length = from[6..8].try_into()?;

        let request = ERequest::try_from((
            packed_request_type.recipient(),
            packed_request_type.request_type(),
            from[1],
            u16::from_le_bytes(value),
            u16::from_le_bytes(index),
        ))?;

        Ok(Self {
            direction: packed_request_type.direction(),
            request,
            length: u16::from_le_bytes(length),
        })
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum StandardRequest {
    GetStatus,
    ClearFeature,
    SetFeature,
    SetAddress,
    GetDescriptor(GetDescriptorType),
    SetDescriptor,
    GetConfiguration,
    SetConfiguration(u16),
    GetInterface,
    SetInterface,
    SyncFrame,
    Reserved { request: u8, value: u16, index: u16 },
}
impl TryFrom<(u8, u16, u16)> for StandardRequest {
    type Error = anyhow::Error;

    fn try_from(value: (u8, u16, u16)) -> Result<Self, Self::Error> {
        let (request, value, index) = value;

        Ok(match request {
            0 => Self::GetStatus,
            1 => Self::ClearFeature,
            3 => Self::SetFeature,
            5 => Self::SetAddress,
            6 => Self::GetDescriptor(GetDescriptorType::try_from((value, index))?),
            7 => Self::SetDescriptor,
            8 => Self::GetConfiguration,
            9 => Self::SetConfiguration(value),
            10 => Self::GetInterface,
            11 => Self::SetInterface,
            12 => Self::SyncFrame,
            _ => Self::Reserved {
                request,
                value,
                index,
            },
        })
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum GetDescriptorType {
    Device,
    Configuration(u8),
    String(u8, LanguageId),
    DeviceQualifier,
    BinaryObjectStore(u8, u16),
    OtherSpeedConfiguration,
}
impl TryFrom<(u16, u16)> for GetDescriptorType {
    type Error = anyhow::Error;

    fn try_from(value: (u16, u16)) -> Result<Self, Self::Error> {
        let (value, index) = value;
        let value_high = (value >> 8) as u8;
        let value_low = (value & 0xFF) as u8;

        Ok(match (value_high, value_low, index) {
            (DEVICE_DESCRIPTOR, 0, 0) => Self::Device,
            (CONFIGURATION_DESCRIPTOR, value_low, 0) => Self::Configuration(value_low),
            (STRING_DESCRIPTOR, value_low, index) => {
                Self::String(value_low, LanguageId::from(index))
            }
            (DEVICE_QUALIFIER_DESCRIPTOR, 0, 0) => Self::DeviceQualifier,
            (OTHER_SPEED_CONFIGURATION_DESCRIPTOR, 0, 0) => Self::OtherSpeedConfiguration,
            (BINARY_OBJECT_STORE_DESCRIPTOR, _, _) => Self::BinaryObjectStore(value_low, index),
            (_, _, _) => anyhow::bail!("Invalid descriptor type {}", value_high),
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MaxPacketSize {
    _8,
    _16,
    _32,
    _64,
    Reserved(u8),
}
impl From<u8> for MaxPacketSize {
    fn from(value: u8) -> Self {
        match value {
            8 => Self::_8,
            16 => Self::_16,
            32 => Self::_32,
            64 => Self::_64,
            _ => Self::Reserved(value),
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct UsbVersion(pub u16);
impl std::fmt::Debug for UsbVersion {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}.{}.{}",
            self.0 >> 8,
            (self.0 >> 4) & 0xF,
            self.0 & 0xF
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MiscellaneousSubClass {
    InterfaceAssociationDescriptor,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeviceClass {
    CommunicationDevice(cdc::DeviceSubClass),
    Miscellaneous(MiscellaneousSubClass),
}

impl TryFrom<(u8, u8, u8)> for DeviceClass {
    type Error = anyhow::Error;

    fn try_from(class_code_triple: (u8, u8, u8)) -> Result<Self, Self::Error> {
        Ok(match class_code_triple {
            (2, subclass, protocol) => DeviceClass::CommunicationDevice(
                cdc::DeviceSubClass::try_from((subclass, protocol))?,
            ),
            (0xEF, 2, 1) => {
                DeviceClass::Miscellaneous(MiscellaneousSubClass::InterfaceAssociationDescriptor)
            }
            _ => anyhow::bail!("Unsupported device class triple {:?}", class_code_triple),
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DeviceCapabilityDescriptor {
    USB20Extensions { link_power_management: bool },
    Unimplemented(u8, Vec<u8>),
    Reserved(u8, Vec<u8>),
}
impl DeviceCapabilityDescriptor {
    fn parse(buffer: &[u8]) -> anyhow::Result<(&[u8], Self)> {
        let (desc_length, desc_type) = buffer
            .iter()
            .cloned()
            .tuples()
            .next()
            .ok_or_else(|| anyhow::anyhow!("Truncated descriptor"))?;
        let desc_length: usize = desc_length.into();

        anyhow::ensure!(
            desc_type == DEVICE_CAPABILITY_DESCRIPTOR,
            "Invalid descriptor type (expected device capability got {})",
            desc_type
        );
        anyhow::ensure!(
            desc_length <= buffer.len(),
            "Insuficent data available length (expected {} got {})",
            desc_length,
            buffer.len()
        );

        let device_capability_type = buffer[2];
        Ok((
            &buffer[desc_length..],
            match device_capability_type {
                0 | 0x11..=0xFF => {
                    Self::Reserved(device_capability_type, buffer[3..desc_length].to_vec())
                }
                2 => {
                    anyhow::ensure!(
                    desc_length == 7,
                    "Invalid USB 2.0 Extension capability descriptor length (expected 7 got {})",
                    desc_length
                );

                    let attributes = u32::from_le_bytes(buffer[3..7].try_into()?);
                    let link_power_management = match attributes {
                        0 => false,
                        0x0000_0002 => true,
                        _ => anyhow::bail!("Invalid USB 2.0 Extension attributes value"),
                    };

                    Self::USB20Extensions {
                        link_power_management,
                    }
                }
                _ => Self::Unimplemented(device_capability_type, buffer[3..desc_length].to_vec()),
            },
        ))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BinaryObjectStore(pub Vec<DeviceCapabilityDescriptor>);
impl BinaryObjectStore {
    fn parse(buffer: &[u8]) -> anyhow::Result<Self> {
        let (desc_length, desc_type) = buffer
            .iter()
            .cloned()
            .tuples()
            .next()
            .ok_or_else(|| anyhow::anyhow!("Truncated descriptor"))?;

        anyhow::ensure!(
            desc_type == BINARY_OBJECT_STORE_DESCRIPTOR,
            "Invalid descriptor type (expected binary object store got {})",
            desc_type
        );
        anyhow::ensure!(
            desc_length == 5,
            "Invalid binary object store descriptor length (expected 5 got {})",
            desc_length
        );

        let total_length: usize = u16::from_le_bytes(buffer[2..4].try_into()?).into();
        let num_caps = buffer[4].into();

        anyhow::ensure!(
            total_length == buffer.len(),
            "Truncated Binary Object Store"
        );

        let mut read_ptr = &buffer[5..];
        let mut capabilities = Vec::with_capacity(num_caps);
        for _ in 0..num_caps {
            let (new_read_ptr, capability_desc) = DeviceCapabilityDescriptor::parse(read_ptr)?;
            read_ptr = new_read_ptr;
            capabilities.push(capability_desc);
        }

        Ok(BinaryObjectStore(capabilities))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct DeviceRelease(pub u16);

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Descriptor {
    Device(DeviceDescriptor),
    String(StringDescriptor),
    Configuration(ConfigurationDescriptor),
    BinaryObjectStore(BinaryObjectStore),
    Reserved(Vec<u8>),
}

impl TryFrom<(GetDescriptorType, Vec<u8>)> for Descriptor {
    type Error = anyhow::Error;

    fn try_from(
        (req_desc_type, response): (GetDescriptorType, Vec<u8>),
    ) -> Result<Self, Self::Error> {
        let (rsp_desc_len, rsp_desc_type) = response
            .iter()
            .cloned()
            .tuples()
            .next()
            .ok_or_else(|| anyhow::anyhow!("Truncated descriptor"))?;

        Ok(match (req_desc_type, rsp_desc_type) {
            (GetDescriptorType::Device, DEVICE_DESCRIPTOR) => {
                let desc_buf: [u8; 18] = response
                    .get(..rsp_desc_len.into())
                    .ok_or_else(|| anyhow::anyhow!("Truncated device descriptor"))?
                    .try_into()?;
                Descriptor::Device(DeviceDescriptor::try_from(desc_buf)?)
            }
            (GetDescriptorType::Configuration(_), CONFIGURATION_DESCRIPTOR) => {
                let configuration = ConfigurationDescriptor::parse(&response)?;
                Descriptor::Configuration(configuration)
            }
            (GetDescriptorType::String(index, _), STRING_DESCRIPTOR) => {
                let string = StringDescriptor::parse(index, &response)?;
                Descriptor::String(string)
            }
            (GetDescriptorType::BinaryObjectStore(_, _), BINARY_OBJECT_STORE_DESCRIPTOR) => {
                let bos = BinaryObjectStore::parse(&response)?;
                Descriptor::BinaryObjectStore(bos)
            }
            (_, 4..=8) => anyhow::bail!(
                "Unsupported descriptor type {} in {:?}",
                rsp_desc_type,
                response
            ),
            (_, _) => Self::Reserved(response),
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DeviceDescriptor {
    usb_version: UsbVersion,
    device_class: DeviceClass,
    max_packet_size: MaxPacketSize,
    vendor_id: u16,
    product_id: u16,
    device_release_number: DeviceRelease,
    manufacturer_string_index: u8,
    product_string_index: u8,
    serial_number_string_index: u8,
    num_configuration: u8,
}
impl TryFrom<[u8; 18]> for DeviceDescriptor {
    type Error = anyhow::Error;

    fn try_from(response: [u8; 18]) -> Result<Self, Self::Error> {
        let (desc_length, desc_type) = response
            .iter()
            .cloned()
            .tuples()
            .next()
            .ok_or_else(|| anyhow::anyhow!("Truncated descriptor"))?;

        anyhow::ensure!(
            desc_type == DEVICE_DESCRIPTOR,
            "Invalid descriptor type (expected device got {})",
            desc_type
        );
        anyhow::ensure!(
            desc_length == 18,
            "Invalid device descriptor length (expected 18 got {})",
            desc_length
        );

        let usb_version = response[2..4]
            .try_into()
            .map(u16::from_le_bytes)
            .map(UsbVersion)?;
        let device_class = DeviceClass::try_from((response[4], response[5], response[6]))?;
        let max_packet_size = MaxPacketSize::try_from(response[7])?;
        let vendor_id = response[8..10].try_into().map(u16::from_le_bytes)?;
        let product_id = response[10..12].try_into().map(u16::from_le_bytes)?;
        let device_release_number = response[12..14]
            .try_into()
            .map(u16::from_le_bytes)
            .map(DeviceRelease)?;
        let manufacturer_string_index = response[14];
        let product_string_index = response[15];
        let serial_number_string_index = response[16];
        let num_configuration = response[17];

        Ok(DeviceDescriptor {
            usb_version,
            device_class,
            max_packet_size,
            vendor_id,
            product_id,
            device_release_number,
            manufacturer_string_index,
            product_string_index,
            serial_number_string_index,
            num_configuration,
        })
    }
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StringDescriptor {
    CodeArray(Vec<LanguageId>),
    String(String),
}
impl StringDescriptor {
    fn parse(index: u8, response: &[u8]) -> anyhow::Result<Self> {
        let (desc_length, desc_type) = response
            .iter()
            .cloned()
            .tuples()
            .next()
            .ok_or_else(|| anyhow::anyhow!("Truncated descriptor"))?;

        anyhow::ensure!(
            desc_type == STRING_DESCRIPTOR,
            "Invalid descriptor type (expected string got {})",
            desc_type
        );
        anyhow::ensure!(
            usize::from(desc_length) == response.len(),
            "Truncated string descriptor {:x?}",
            response
        );

        let vec: Vec<_> = response[2..]
            .chunks(2)
            .filter_map(|chunk| chunk.try_into().ok())
            .map(u16::from_le_bytes)
            .collect();
        Ok(match index {
            0 => StringDescriptor::CodeArray(
                vec.into_iter().map(LanguageId::from).collect::<Vec<_>>(),
            ),
            _ => StringDescriptor::String(String::from_utf16_lossy(&vec)),
        })
    }
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct MaxPower(pub u8);
impl std::fmt::Debug for MaxPower {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "\"{}mA\"", u16::from(self.0) * 2)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ConfigurationCharacteristics {
    self_powered: bool,
    remote_wakeup: bool,
}
impl TryFrom<u8> for ConfigurationCharacteristics {
    type Error = anyhow::Error;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        anyhow::ensure!(value & 0x80 == 0x80, "Invalid D7 field (should be set)");
        anyhow::ensure!(
            value & 0x1F == 0x00,
            "Invalid D4..D0 fields (should be cleared)"
        );
        Ok(ConfigurationCharacteristics {
            self_powered: value & 0x40 == 0x40,
            remote_wakeup: value & 0x20 == 0x20,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfigurationDescriptor {
    configuration_value: u8,
    description_string_index: u8,
    interfaces: Vec<InterfaceDescriptor>,
    attributes: ConfigurationCharacteristics,
    max_power: MaxPower,
}

impl ConfigurationDescriptor {
    fn parse(response: &[u8]) -> anyhow::Result<Self> {
        let (desc_length, desc_type) = response
            .iter()
            .cloned()
            .tuples()
            .next()
            .ok_or_else(|| anyhow::anyhow!("Truncated descriptor"))?;

        anyhow::ensure!(
            desc_type == CONFIGURATION_DESCRIPTOR,
            "Invalid descriptor type (expected configuration got {})",
            desc_type
        );
        anyhow::ensure!(
            desc_length == 9 && response.len() >= 9,
            "Truncated configuration descriptor {:x?}",
            response
        );

        let _total_length = response[2..4].try_into().map(u16::from_le_bytes)?;
        let num_interfaces = response[4];
        anyhow::ensure!(
            response.len() == usize::from(_total_length),
            "Truncated descriptor list {:?}",
            response
        );

        // nom_parser for interfaces ?
        let mut interfaces = Vec::with_capacity(num_interfaces.into());
        let mut read_ptr = &response[9..];

        while !read_ptr.is_empty() {
            let (new_read_ptr, interface) = InterfaceDescriptor::parse(read_ptr).map_err(|e| {
                println!("{:?} --- {:?}", interfaces.len(), e);
                e
            })?;
            read_ptr = new_read_ptr;
            interfaces.push(interface);
        }

        Ok(ConfigurationDescriptor {
            configuration_value: response[5],
            description_string_index: response[6],
            interfaces,
            attributes: ConfigurationCharacteristics::try_from(response[7])?,
            max_power: MaxPower(response[8]),
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClassSpecificDescriptor {
    CommunicationDevice(cdc::ClassSpecificDescriptor),
    MassStorageDevice(msd::ClassSpecificDescriptor),
    Other(Vec<u8>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InterfaceClass {
    CommunicationDevice(cdc::InterfaceSubClass),
    CDCData(cdc::DataInterfaceSubClass),
    MassStorageDevice(msd::InterfaceSubClass),
    VendorSpecific { subclass: u8, protocol: u8 },
}

impl InterfaceClass {
    fn parse_descriptor<'descriptor>(
        &self,
        descriptor: &'descriptor [u8],
    ) -> anyhow::Result<(&'descriptor [u8], ClassSpecificDescriptor)> {
        Ok(match self {
            Self::CommunicationDevice(_) => {
                let (next, desc) = cdc::ClassSpecificDescriptor::parse(descriptor)?;
                (next, ClassSpecificDescriptor::CommunicationDevice(desc))
            }
            Self::MassStorageDevice(_) => {
                let (next, desc) = msd::ClassSpecificDescriptor::parse(descriptor)?;
                (next, ClassSpecificDescriptor::MassStorageDevice(desc))
            }
            _ => anyhow::bail!("Class specific descriptor not implemented for {:?}", self),
        })
    }
}

impl TryFrom<(u8, u8, u8)> for InterfaceClass {
    type Error = anyhow::Error;

    fn try_from(value: (u8, u8, u8)) -> Result<Self, Self::Error> {
        let (class, subclass, protocol) = value;

        Ok(match class {
            2 => Self::CommunicationDevice(cdc::InterfaceSubClass { subclass, protocol }),
            8 => Self::MassStorageDevice(msd::InterfaceSubClass { subclass, protocol }),
            10 => Self::CDCData(cdc::DataInterfaceSubClass { subclass, protocol }),
            0xFF => Self::VendorSpecific { subclass, protocol },
            _ => anyhow::bail!("Unsupported interface class: {}", class),
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InterfaceDescriptor {
    Plain(PlainInterfaceDescriptor),
    Association(InterfaceAssociationDescriptor),
}
impl InterfaceDescriptor {
    fn parse(response: &[u8]) -> anyhow::Result<(&[u8], Self)> {
        let desc_type = *response
            .get(1)
            .ok_or_else(|| anyhow::anyhow!("Truncated interface descriptor"))?;

        Ok(match desc_type {
            INTERFACE_DESCRIPTOR => {
                let (read_ptr, interface) = PlainInterfaceDescriptor::parse(response)?;
                    (read_ptr, InterfaceDescriptor::Plain(interface))
            }
            INTERFACE_ASSOCIATION_DESCRIPTOR => {
                let (read_ptr, interface)= InterfaceAssociationDescriptor::parse(response)?;
                    (read_ptr, InterfaceDescriptor::Association(interface))
                }
            _ => anyhow::bail!(
                "Unexpected descriptor type {} when expecting Interface or Interface Association Descriptor",
                desc_type
            )
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlainInterfaceDescriptor {
    id: u8,
    alternate_setting: u8,
    endpoints: Vec<EndpointDescriptor>,
    interface_class_descriptor: Vec<ClassSpecificDescriptor>,
    class: InterfaceClass,
    description_string_index: u8,
}
impl PlainInterfaceDescriptor {
    fn parse(response: &[u8]) -> anyhow::Result<(&[u8], Self)> {
        let (desc_length, desc_type) = response
            .iter()
            .cloned()
            .tuples()
            .next()
            .ok_or_else(|| anyhow::anyhow!("Truncated descriptor"))?;

        anyhow::ensure!(
            desc_type == INTERFACE_DESCRIPTOR,
            "Invalid descriptor type (expected interface got {})",
            desc_type
        );
        anyhow::ensure!(
            desc_length == 9 && response.len() >= 9,
            "Truncated interface descriptor {:x?}",
            response
        );

        let id = response[2];
        let alternate_setting = response[3];
        let num_endpoints = response[4];
        let class = InterfaceClass::try_from((response[5], response[6], response[7]))?;
        let description_string_index = response[8];

        let mut read_ptr = &response[9..];
        let mut interface_class_descriptor = Vec::new();
        loop {
            let desc_type = *read_ptr
                .get(1)
                .ok_or_else(|| anyhow::anyhow!("Truncated descriptor"))?;

            if desc_type == ENDPOINT_DESCRIPTOR {
                break;
            }

            let (new_resp_ptr, descriptor) = class.parse_descriptor(read_ptr)?;
            interface_class_descriptor.push(descriptor);
            read_ptr = new_resp_ptr;
        }
        let mut endpoints = Vec::with_capacity(num_endpoints.into());

        for _ in 0..num_endpoints {
            let len = read_ptr.get(0).unwrap_or(&0).clone().into();
            let desc: [u8; 7] = read_ptr[..len].try_into()?;
            let endpoint = EndpointDescriptor::try_from(desc)?;
            endpoints.push(endpoint);
            read_ptr = &read_ptr[desc.len()..];
        }

        Ok((
            read_ptr,
            PlainInterfaceDescriptor {
                id,
                alternate_setting,
                interface_class_descriptor,
                endpoints,
                class,
                description_string_index,
            },
        ))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InterfaceAssociationDescriptor {
    first_interface: u8,
    interfaces: Vec<PlainInterfaceDescriptor>,
    function_class: (u8, u8, u8),
    function_description_string_index: u8,
}
impl InterfaceAssociationDescriptor {
    fn parse(response: &[u8]) -> anyhow::Result<(&[u8], Self)> {
        let (desc_length, desc_type) = response
            .iter()
            .cloned()
            .tuples()
            .next()
            .ok_or_else(|| anyhow::anyhow!("Truncated descriptor"))?;

        anyhow::ensure!(
            desc_type == 11,
            "Invalid descriptor type (expected interface association got {})",
            desc_type
        );
        anyhow::ensure!(
            desc_length == 8 && response.len() >= 8,
            "Truncated interface association descriptor {:x?}",
            response
        );

        let first_interface = response[2];
        let interface_count = response[3];
        let function_class = response[4];
        let function_subclass = response[5];
        let function_protocol = response[6];
        let function_description_string_index = response[7];

        let mut interfaces = Vec::with_capacity(interface_count.into());
        let mut next_descriptor = &response[8..];

        for _ in 0..interface_count {
            let (new_read_ptr, interface) = PlainInterfaceDescriptor::parse(next_descriptor)?;
            next_descriptor = new_read_ptr;
            interfaces.push(interface);
        }
        Ok((
            next_descriptor,
            InterfaceAssociationDescriptor {
                first_interface,
                interfaces,
                function_class: (function_class, function_subclass, function_protocol),
                function_description_string_index,
            },
        ))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EndpointDirection {
    In,
    Out,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SyncType {
    NoSynchronization,
    Asynchronous,
    Adaptive,
    Synchronous,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UsageType {
    DataEndpoint,
    FeedbackEndpoint,
    ImplicitFeedbackDataEndpoint,
    Reserved,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransferType {
    Control,
    Isochronous {
        sync_type: SyncType,
        usage_type: UsageType,
    },
    Bulk,
    Interrupt,
}
impl TryFrom<u8> for TransferType {
    type Error = anyhow::Error;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        let transfer_type = value & 0x03;
        Ok(if transfer_type == 1 {
            let sync_type = (value & 0x0C) >> 2;
            let usage_type = (value & 0x30) >> 4;

            TransferType::Isochronous {
                sync_type: match sync_type {
                    0 => SyncType::NoSynchronization,
                    1 => SyncType::Asynchronous,
                    2 => SyncType::Adaptive,
                    3 => SyncType::Synchronous,
                    _ => unreachable!(),
                },
                usage_type: match usage_type {
                    0 => UsageType::DataEndpoint,
                    1 => UsageType::FeedbackEndpoint,
                    2 => UsageType::ImplicitFeedbackDataEndpoint,
                    3 => UsageType::Reserved,
                    _ => unreachable!(),
                },
            }
        } else {
            anyhow::ensure!(
                transfer_type & 0xFC == 0,
                "Invalid reserved bits {:8b}",
                transfer_type & 0xFC
            );
            match transfer_type {
                0 => TransferType::Control,
                2 => TransferType::Bulk,
                3 => TransferType::Interrupt,
                _ => unreachable!(),
            }
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EndpointDescriptor {
    endpoint_number: u8,
    direction: EndpointDirection,
    attributes: TransferType,
    max_packet_size: u16,
    interval: u8,
}
impl TryFrom<[u8; 7]> for EndpointDescriptor {
    type Error = anyhow::Error;

    fn try_from(response: [u8; 7]) -> Result<Self, Self::Error> {
        let desc_length = response[0];
        let desc_type = response[1];

        anyhow::ensure!(
            desc_type == 5,
            "Invalid descriptor type (expected endpoint({}) got {})",
            ENDPOINT_DESCRIPTOR,
            desc_type
        );
        anyhow::ensure!(
            desc_length == 7,
            "Invalid descriptor length (expected 7 got {})",
            desc_length
        );

        let endpoint_address = response[2];
        anyhow::ensure!(
            endpoint_address & 0x70 == 0,
            "Invalid reserved bit value {:08b} (should be cleared)",
            endpoint_address & 0x70
        );

        let endpoint_number = endpoint_address & 0x0F;
        let direction = if endpoint_address & 0x80 == 0 {
            EndpointDirection::Out
        } else {
            EndpointDirection::In
        };

        let max_packet_size = response[4..6].try_into().map(u16::from_le_bytes)?;
        let interval = response[6];

        Ok(EndpointDescriptor {
            endpoint_number,
            direction,
            attributes: TransferType::try_from(response[3])?,
            max_packet_size,
            interval,
        })
    }
}
