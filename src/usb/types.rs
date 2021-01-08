use std::convert::TryFrom;
use std::fmt::Debug;

#[derive(PartialEq, Debug, Copy, Clone)]
pub enum RequestType {
    Standard,
    Class,
    Vendor,
    Reserved,
}

#[derive(PartialEq, Debug, Copy, Clone)]
pub enum Recipient {
    Device,
    Interface,
    Endpoint,
    Other,
    Reserved(u8),
}

#[derive(PartialEq, Debug, Copy, Clone)]
pub enum DataPhaseTransferDirection {
    In,
    Out,
}

#[derive(PartialEq, Copy, Clone)]
pub struct PackedRequestType(u8);
impl PackedRequestType {
    pub fn direction(&self) -> DataPhaseTransferDirection {
        if (self.0 & 0x80) == 0x80 {
            DataPhaseTransferDirection::In
        } else {
            DataPhaseTransferDirection::Out
        }
    }

    pub fn request_type(&self) -> RequestType {
        let ptype = (self.0 >> 5) & 3;
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
impl Debug for PackedRequestType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PackedRequestType")
            .field("direction", &self.direction())
            .field("request_type", &self.request_type())
            .field("recipient", &self.recipient())
            .finish()
    }
}

#[derive(Debug, Copy, Clone, PartialEq)]
pub enum TokenType {
    Setup,
    Out,
    In,
    Ping,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Token {
    pub token_type: TokenType,
    pub address: u8,
    pub endpoint: u8,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum DataPID {
    Data0,
    Data1,
    Data2,
    MData,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Data {
    pub pid: DataPID,
    pub payload: Vec<u8>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum HandShake {
    Ack,
    NAck,
    Stall,
    NYet,
    Err,
}

pub fn crc5(v: &[u8]) -> u8 {
    let mut acc = 0x1F;
    for b in v {
        let mut b = *b;
        for _ in 0..8 {
            let do_xor = (b & 1) != ((acc >> 4) & 1);
            acc <<= 1;
            if do_xor {
                acc ^= 5
            }
            acc &= 0x1F;
            b >>= 1;
        }
    }
    acc
}

pub fn crc16(v: &[u8]) -> u16 {
    let mut acc = 0xFFFF;
    for b in v {
        let mut b = *b;
        for _ in 0..8 {
            let do_xor = (b as u16 & 1) != ((acc >> 15) & 1);
            acc <<= 1;
            if do_xor {
                acc ^= 0x8005
            }
            b >>= 1;
        }
    }
    acc
}

#[derive(Copy, Clone)]
pub struct GetDescriptorValue(u16);
impl Debug for GetDescriptorValue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GetDescriptorValue")
            .field(
                "descriptor_type",
                &DescriptorType::try_from((self.0 >> 8) as u8),
            )
            .field("descriptor_index", &(self.0 & 0xFF))
            .finish()
    }
}

#[derive(PartialEq, Clone, Copy)]
pub struct DeviceRequest {
    pub request_type: PackedRequestType,
    pub request: u8,
    pub value: u16,
    pub index: u16,
    pub length: u16,
}
impl Debug for DeviceRequest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let default_to = |me: &Self| -> (Box<dyn Debug>, Box<dyn Debug>) {
            (Box::new(me.request), Box::new(me.index))
        };
        let req = if self.request_type.request_type() == RequestType::Standard {
            match StandardRequest::try_from(self.request) {
                Ok(v @ StandardRequest::GetDescriptor) => (
                    Box::new(v) as Box<dyn Debug>,
                    Box::new(GetDescriptorValue(self.value)) as Box<dyn Debug>,
                ),
                Ok(v @ StandardRequest::SetAddress) => (
                    Box::new(v) as Box<dyn Debug>,
                    Box::new(self.value) as Box<dyn Debug>,
                ),
                _ => default_to(self),
            }
        } else {
            default_to(self)
        };

        f.debug_struct("DeviceRequest")
            .field("direction", &self.request_type.direction())
            .field("request_type", &self.request_type.request_type())
            .field("recipient", &self.request_type.recipient())
            .field("request", &req.0)
            .field("value", &req.1)
            .field("index", &self.index)
            .field("length", &self.length)
            .finish()
    }
}
impl TryFrom<&[u8]> for DeviceRequest {
    type Error = anyhow::Error;
    fn try_from(from: &[u8]) -> Result<DeviceRequest, Self::Error> {
        anyhow::ensure!(from.len() == 8, "Invalid Setup packet length.");

        Ok(Self {
            request_type: PackedRequestType(from[0]),
            request: from[1],
            value: ((from[3] as u16) << 8) | (from[2] as u16),
            index: ((from[5] as u16) << 8) | (from[4] as u16),
            length: ((from[7] as u16) << 8) | (from[6] as u16),
        })
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum StandardRequest {
    GetStatus,
    ClearFeature,
    SetFeature,
    SetAddress,
    GetDescriptor,
    SetDescriptor,
    GetConfiguration,
    SetConfiguration,
    GetInterface,
    SetInterface,
    SyncFrame,
}
impl TryFrom<u8> for StandardRequest {
    type Error = anyhow::Error;
    fn try_from(value: u8) -> Result<Self, Self::Error> {
        Ok(match value {
            0 => StandardRequest::GetStatus,
            1 => StandardRequest::ClearFeature,
            3 => StandardRequest::SetFeature,
            5 => StandardRequest::SetAddress,
            6 => StandardRequest::GetDescriptor,
            7 => StandardRequest::SetDescriptor,
            8 => StandardRequest::GetConfiguration,
            9 => StandardRequest::SetConfiguration,
            10 => StandardRequest::GetInterface,
            11 => StandardRequest::SetInterface,
            12 => StandardRequest::SyncFrame,
            _ => anyhow::bail!("Invalid standard request: {}", value),
        })
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum DescriptorType {
    Device,
    Configuration,
    String,
    Interface,
    Endpoint,
    DeviceQualifier,
    OtherSpeedConfiguration,
    InterfacePower,
}
impl TryFrom<u8> for DescriptorType {
    type Error = anyhow::Error;
    fn try_from(value: u8) -> Result<Self, Self::Error> {
        Ok(match value {
            1 => DescriptorType::Device,
            2 => DescriptorType::Configuration,
            3 => DescriptorType::String,
            4 => DescriptorType::Interface,
            5 => DescriptorType::Endpoint,
            6 => DescriptorType::DeviceQualifier,
            7 => DescriptorType::OtherSpeedConfiguration,
            8 => DescriptorType::InterfacePower,
            _ => anyhow::bail!("Invalid descriptor type: {}", value),
        })
    }
}
